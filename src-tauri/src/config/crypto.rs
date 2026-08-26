//! crypto：DPAPI 加解密（spec 5.7.8，T3）。
//!
//! - `protect(plain)`：`CryptProtectData`（CurrentUser 缺省作用域 + 应用常量熵 + 禁止 UI 弹窗）；
//! - `unprotect(cipher)`：`CryptUnprotectData` 反向；
//! - 密文以 base64 文本进出，落盘于 config.json 的 `ai.apiKeyCipher`（INV-1：磁盘只有密文）；
//! - 解密失败返回 `Err`，由上层降级为"未配置"（`config::has_key`，spec 5.7.8：不崩溃、
//!   不弹系统级错误）；本模块只负责分类与传播。
//!
//! 威胁模型如实记录（spec 5.7.8）：CurrentUser 作用域把密文绑定本机当前 Windows 用户账户；
//! 应用常量熵仅提高"跨应用拷贝密文"的门槛——同用户恶意程序本就能解 DPAPI 用户态密文，
//! 超出 v1 威胁模型，故不引入额外密钥管理（密码保护/本地派生密钥）开销。

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// 应用常量熵：仅提高跨应用拷贝门槛（与上文件风险说明一致，非可解密性支撑）。
const ENTROPY: &[u8] = b"alya::api-key:v1";

/// 本模块错误分类（无任何值内容，供上层按类别处置/记日志；INV-7）。
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("密文无法解出（base64 解码失败，密文损坏或非本应用产物）")]
    Base64,
    #[error("DPAPI 加密失败: {0}")]
    Protect(windows::core::Error),
    #[error("DPAPI 解密失败（被篡改/跨用户账户/密码重置后密文失效）: {0}")]
    Unprotect(windows::core::Error),
    #[error("输入超过 DPAPI 单块上限")]
    TooLong,
    #[error("DPAPI 返回空数据")]
    EmptyBlob,
    #[error("解密结果不是合法 UTF-8 文本（凭据损坏）")]
    NotUtf8,
}

impl CryptoError {
    /// 无值错误类别标识（供"记无值日志"使用，spec 5.7.8；不含密文/明文任何字节）。
    pub fn class(&self) -> &'static str {
        match self {
            CryptoError::Base64 => "base64",
            CryptoError::Protect(_) => "protect",
            CryptoError::Unprotect(_) => "unprotect",
            CryptoError::TooLong => "too_long",
            CryptoError::EmptyBlob => "empty_blob",
            CryptoError::NotUtf8 => "not_utf8",
        }
    }
}

/// 加密明文（字节），返回 base64 密文文本（可直接存入 `ai.apiKeyCipher`）。
pub fn protect(plain: &[u8]) -> Result<String, CryptoError> {
    let cb_data = u32::try_from(plain.len()).map_err(|_| CryptoError::TooLong)?;
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: cb_data,
        pbData: plain.as_ptr() as *mut u8,
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };

    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    // SAFETY：调用期间 input/entropy 缓冲区均有效（借用不释放）；szdatadescr 传空（无描述）；
    // dwflags 禁止 UI 弹窗（降级路径不允许出现系统错误框）；out_blob 由下方 LocalFree 释放。
    let result = unsafe {
        CryptProtectData(
            &input_blob,
            PCWSTR::null(),
            Some(&entropy_blob as *const CRYPT_INTEGER_BLOB),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    if let Err(err) = result {
        return Err(CryptoError::Protect(err));
    }
    // 读取输出
    let cipher = if out_blob.cbData == 0 || out_blob.pbData.is_null() {
        // 防御性分支：成功返回空 BLOB 视为异常（正常数据至少非零长）。
        unsafe { free_blob(&mut out_blob) };
        return Err(CryptoError::EmptyBlob);
    } else {
        // SAFETY：out_blob.cbData 为（成功返回时）有效字节数，pbData 指向合法内存块。
        let bytes =
            unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) };
        STANDARD.encode(bytes)
    };
    unsafe { free_blob(&mut out_blob) };
    Ok(cipher)
}

/// 解密 base64 密文为明文；失败返回 `Err`（上层降级，见模块头注释）。
pub fn unprotect(cipher: &str) -> Result<Vec<u8>, CryptoError> {
    let blob_bytes = STANDARD
        .decode(cipher)
        .map_err(|_err| CryptoError::Base64)?;
    let cb_data = u32::try_from(blob_bytes.len()).map_err(|_| CryptoError::TooLong)?;
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: cb_data,
        pbData: blob_bytes.as_ptr() as *mut u8,
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };

    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    // SAFETY：与 protect 相同的借用/释放契约；ppszdatadescr 传 None（不需要描述字符串）。
    let result = unsafe {
        CryptUnprotectData(
            &input_blob,
            None,
            Some(&entropy_blob as *const CRYPT_INTEGER_BLOB),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    if let Err(err) = result {
        return Err(CryptoError::Unprotect(err));
    }
    if out_blob.cbData == 0 || out_blob.pbData.is_null() {
        unsafe { free_blob(&mut out_blob) };
        return Err(CryptoError::EmptyBlob);
    }
    // SAFETY：成功返回时 out_blob 描述的数据块有效；读取后立即释放。
    let plain =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    unsafe { free_blob(&mut out_blob) };
    // 注：明文 Vec 不引入 zeroize——v1 威胁模型无内存取证需求；值随作用域结束释放（模块头）。
    Ok(plain)
}

/// 解密 base64 密文为 UTF-8 文本（API Key 等文本凭据进出；T8 的 sse/verify 消费形态）。
/// 失败同 `unprotect`（`NotUtf8`＝凭据字节非文本，判为损坏，上层一律降级"未配置"）。
pub fn unprotect_str(cipher: &str) -> Result<String, CryptoError> {
    let bytes = unprotect(cipher)?;
    String::from_utf8(bytes).map_err(|_| CryptoError::NotUtf8)
}

/// 释放 DPAPI 返回的 BLOB（CryptProtectData/CryptUnprotectData 约定以 LocalFree 释放）。
///
/// # Safety
/// `blob.pbData` 必须是上述函数成功返回后、未经任何转移的指针，且本函数只调用一次。
unsafe fn free_blob(blob: &mut CRYPT_INTEGER_BLOB) {
    if !blob.pbData.is_null() {
        // SAFETY：HLOCAL 承袭自 DPAPI 输出指针；LocalFree 对有效堆指针恒成功（返回 NULL）。
        unsafe {
            LocalFree(Some(HLOCAL(blob.pbData as *mut core::ffi::c_void)));
        }
        blob.pbData = std::ptr::null_mut();
        blob.cbData = 0;
    }
}

// ---------------------------------------------------------------------------
// 单元测试：加解密往返 / 篡改失败 / 降级分支（Windows 真机直接调用真实 DPAPI 函数）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::{has_key, AiSettings, Settings};

    /// 无 unwrap/expect 的取值辅助（INV-6）。
    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功值，实际错误: {err}"),
        }
    }

    #[test]
    fn protect_unprotect_roundtrip() {
        // 哑数据（非真实凭据；凭据纪律 K3/J4——永不使用真实 Key）
        let long_plain = b"a".repeat(2000);
        for plain in [b"sk-fake-test-key-000".as_slice(), long_plain.as_slice()] {
            let cipher = ok_of(protect(plain));
            assert!(!cipher.is_empty());
            let restored = ok_of(unprotect(&cipher));
            assert_eq!(restored, plain);
        }
    }

    #[test]
    fn tampered_cipher_fails() {
        let cipher = ok_of(protect(b"sk-fake-test-key-000"));
        // 翻转密文首字符（保持长度）：base64 解码可能成功但内容已变 → 解密必须失败。
        let mut chars: Vec<char> = cipher.chars().collect();
        match chars.get_mut(0) {
            Some(first) => {
                *first = if *first == 'A' { 'B' } else { 'A' };
                let tampered: String = chars.into_iter().collect();
                assert!(
                    unprotect(&tampered).is_err(),
                    "内容被篡改的密文不得解出明文"
                );
            }
            None => panic!("密文不应为空"),
        }
        // 非法 base64（含空密文）必然失败
        assert!(unprotect("").is_err());
        assert!(unprotect("!!!not-base64!!!").is_err());
    }

    #[test]
    fn unprotect_wrong_entropy_fails() {
        // 用非本应用熵保护的数据（模拟其他应用密文），本应用必须解不出。
        let foreign_entropy = CRYPT_INTEGER_BLOB {
            cbData: ENTROPY.len() as u32,
            pbData: b"other-app::entropy".as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB::default();
        let input_blob = CRYPT_INTEGER_BLOB {
            cbData: b"sk-fake-test-key-000".len() as u32,
            pbData: b"sk-fake-test-key-000".as_ptr() as *mut u8,
        };
        let result = unsafe {
            CryptProtectData(
                &input_blob,
                PCWSTR::null(),
                Some(&foreign_entropy as *const CRYPT_INTEGER_BLOB),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        ok_of(result);
        let foreign_cipher = {
            let bytes =
                unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) };
            STANDARD.encode(bytes)
        };
        unsafe { free_blob(&mut out_blob) };
        assert!(
            unprotect(&foreign_cipher).is_err(),
            "不同熵的密文应无法解密（跨应用拷贝门槛）"
        );
    }

    #[test]
    fn degrade_branch_has_key_false() {
        // 未配置：密文缺失 → false
        assert!(!has_key(&Settings::default()));

        // 正常：protect → has_key true（同一进程同一用户上下文，Windows 真机可跑）
        let cipher = ok_of(protect(b"sk-fake-test-key-000"));
        let s = Settings {
            ai: AiSettings {
                api_key_cipher: Some(cipher),
                ..Settings::default().ai
            },
            ..Settings::default()
        };
        assert!(has_key(&s));

        // 降级分支：解不出的密文 → 视同未配置（spec 5.7.8 明文判定：不崩溃不弹出系统错误）
        let broken = Settings {
            ai: AiSettings {
                api_key_cipher: Some("AAAA".to_string()),
                ..Settings::default().ai
            },
            ..Settings::default()
        };
        assert!(!has_key(&broken));
    }
}
