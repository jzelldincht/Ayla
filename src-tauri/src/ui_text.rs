//! 面向用户的壳层文案最小 i18n（i18n-2 九语种）：托盘菜单 / 窗口标题 / 系统通知。
//!
//! 语言解析：config `general.language`——具体语言直取；System 按 Windows UI 语言探测
//! （zh 变体按子语言分简繁，其余按主语言映射，未覆盖 → en）。解析失败 → 中文兜底。
//! 范围说明：仅覆盖原生壳层（非 WebView）文案；WebView 内文案由前端 `src/lib/i18n.svelte.ts` 负责。

use crate::config::{self, Language, Settings, ThemeMode};

/// 系统语言探测（Windows UI 语言 → Language；zh 按子语言分简繁）。
fn detect_system() -> Language {
    use windows::Win32::Globalization::GetUserDefaultUILanguage;
    // SAFETY：无参数系统调用，仅读取 UI 语言标识。
    let id = unsafe { GetUserDefaultUILanguage() };
    let primary = id & 0x00FF;
    let sub = (id >> 10) & 0x003F;
    match primary {
        0x04 => match sub {
            // zh-TW / zh-HK / zh-MO → 繁体；其余 zh → 简体
            0x01 | 0x03 | 0x09 => Language::ZhTw,
            _ => Language::ZhCn,
        },
        0x11 => Language::Ja,
        0x12 => Language::Ko,
        0x0C => Language::Fr,
        0x07 => Language::De,
        0x0A => Language::Es,
        0x19 => Language::Ru,
        _ => Language::En,
    }
}

/// 从 config 解析界面语言（读失败 → 中文兜底；System → 系统探测）。
pub fn resolve(settings: &Settings) -> Language {
    match settings.general.language {
        Language::System => detect_system(),
        other => other,
    }
}

/// 从全局 config 解析（供窗标题/通知等无 settings 上下文的调用点）。
pub fn resolve_global() -> Language {
    config::read_global().map(|s| resolve(&s)).unwrap_or(Language::ZhCn)
}

impl Language {
    /// System 归一为具体语言（resolve 已保证，防御调用点直传 config 值）。
    fn concrete(self) -> Self {
        match self {
            Self::System => Self::ZhCn,
            other => other,
        }
    }
    pub fn tray_scene_menu(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "当前场景",
            Self::ZhTw => "當前場景",
            Self::En => "Scene",
            Self::Ja => "現在のシーン",
            Self::Ko => "현재 장면",
            Self::Fr => "Scène",
            Self::De => "Szene",
            Self::Es => "Escena",
            Self::Ru => "Сцена",
        }
    }
    pub fn tray_scene_mutual(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "互译",
            Self::ZhTw => "互譯",
            Self::En => "Mutual",
            Self::Ja => "相互",
            Self::Ko => "상호",
            Self::Fr => "Mutuelle",
            Self::De => "Gegenseitig",
            Self::Es => "Mutua",
            Self::Ru => "Взаимный",
        }
    }
    pub fn tray_scene_directed(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "定向",
            Self::ZhTw => "定向",
            Self::En => "Directed",
            Self::Ja => "指定",
            Self::Ko => "지정",
            Self::Fr => "Dirigée",
            Self::De => "Gerichtet",
            Self::Es => "Dirigida",
            Self::Ru => "Направленный",
        }
    }
    pub fn tray_input_translate(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "输入翻译",
            Self::ZhTw => "輸入翻譯",
            Self::En => "Input translate",
            Self::Ja => "入力翻訳",
            Self::Ko => "입력 번역",
            Self::Fr => "Traduction par saisie",
            Self::De => "Eingabe übersetzen",
            Self::Es => "Traducir texto escrito",
            Self::Ru => "Перевод ввода",
        }
    }
    pub fn tray_ocr_recognize(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "截图识别",
            Self::ZhTw => "截圖識別",
            Self::En => "Capture text",
            Self::Ja => "キャプチャ認識",
            Self::Ko => "캡처 인식",
            Self::Fr => "Texte par capture",
            Self::De => "Text erfassen",
            Self::Es => "Texto por captura",
            Self::Ru => "Текст со снимка",
        }
    }
    pub fn tray_ocr_translate(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "截图翻译",
            Self::ZhTw => "截圖翻譯",
            Self::En => "Capture translate",
            Self::Ja => "キャプチャ翻訳",
            Self::Ko => "캡처 번역",
            Self::Fr => "Traduction par capture",
            Self::De => "Aufnahme übersetzen",
            Self::Es => "Traducción por captura",
            Self::Ru => "Перевод снимка",
        }
    }
    pub fn tray_clipboard(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "剪贴板监听",
            Self::ZhTw => "剪貼簿監聽",
            Self::En => "Clipboard monitor",
            Self::Ja => "クリップボード監視",
            Self::Ko => "클립보드 모니터",
            Self::Fr => "Presse-papiers",
            Self::De => "Zwischenablage",
            Self::Es => "Portapapeles",
            Self::Ru => "Буфер обмена",
        }
    }
    pub fn tray_settings(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "设置",
            Self::ZhTw => "設定",
            Self::En => "Settings",
            Self::Ja => "設定",
            Self::Ko => "설정",
            Self::Fr => "Paramètres",
            Self::De => "Einstellungen",
            Self::Es => "Ajustes",
            Self::Ru => "Настройки",
        }
    }
    pub fn tray_quit(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "退出",
            Self::ZhTw => "結束",
            Self::En => "Quit",
            Self::Ja => "終了",
            Self::Ko => "종료",
            Self::Fr => "Quitter",
            Self::De => "Beenden",
            Self::Es => "Salir",
            Self::Ru => "Выход",
        }
    }
    pub fn panel_window_title(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "翻译面板",
            Self::ZhTw => "翻譯面板",
            Self::En => "Translation Panel",
            Self::Ja => "翻訳パネル",
            Self::Ko => "번역 패널",
            Self::Fr => "Panneau de traduction",
            Self::De => "Übersetzungspanel",
            Self::Es => "Panel de traducción",
            Self::Ru => "Панель перевода",
        }
    }
    pub fn settings_window_title(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "设置",
            Self::ZhTw => "設定",
            Self::En => "Settings",
            Self::Ja => "設定",
            Self::Ko => "설정",
            Self::Fr => "Paramètres",
            Self::De => "Einstellungen",
            Self::Es => "Ajustes",
            Self::Ru => "Настройки",
        }
    }
    pub fn onboarding_window_title(self) -> &'static str {
        match self.concrete() {
            Self::System | Self::ZhCn => "欢迎使用 Alya",
            Self::ZhTw => "歡迎使用 Alya",
            Self::En => "Welcome to Alya",
            Self::Ja => "Alya へようこそ",
            Self::Ko => "Alya 시작",
            Self::Fr => "Bienvenue dans Alya",
            Self::De => "Willkommen bei Alya",
            Self::Es => "Bienvenido a Alya",
            Self::Ru => "Добро пожаловать в Alya",
        }
    }
    /// D7 焦点降级通知正文（translate.rs）。
    pub fn focus_fallback_notify_body(self) -> String {
        match self.concrete() {
            Self::System | Self::ZhCn => "翻译面板已显示，但未能获得焦点；请点击任务栏图标或按 Alt+E 重新呼出。".to_string(),
            Self::ZhTw => "翻譯面板已顯示，但未能取得焦點；請點擊工作列圖示或按 Alt+E 重新呼出。".to_string(),
            Self::En => "The panel is visible but could not take focus; click its taskbar icon or press Alt+E.".to_string(),
            Self::Ja => "パネルは表示されていますが、フォーカスを取得できませんでした。タスクバーアイコンをクリックするか Alt+E で再表示してください。".to_string(),
            Self::Ko => "패널이 표시되었지만 포커스를 받지 못했습니다. 작업 표시줄 아이콘을 클릭하거나 Alt+E를 누르세요.".to_string(),
            Self::Fr => "Le panneau est visible mais n'a pas pu prendre le focus ; cliquez sur son icône ou appuyez sur Alt+E.".to_string(),
            Self::De => "Das Panel ist sichtbar, konnte aber keinen Fokus erhalten; klicken Sie auf das Tray-Symbol oder drücken Sie Alt+E.".to_string(),
            Self::Es => "El panel es visible pero no pudo tomar el foco; haz clic en su icono o pulsa Alt+E.".to_string(),
            Self::Ru => "Панель видна, но не смогла получить фокус; щёлкните значок на панели задач или нажмите Alt+E.".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 原生标题栏深浅色（真机修复轮 14b 用户需求：标题栏随主题变化）
// ---------------------------------------------------------------------------

/// 系统深色偏好探测（注册表 AppsUseLightTheme=0 → 深色；读失败 → 浅色兜底）。
fn detect_system_dark() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize".encode_utf16()
        .chain([0])
        .collect();
    let name: Vec<u16> = "AppsUseLightTheme".encode_utf16().chain([0]).collect();
    let mut value: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let hr = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as _),
            Some(&mut size),
        )
    };
    hr == ERROR_SUCCESS && value == 0
}

/// 主题 → 是否深色（System 按系统偏好折算）。
pub fn prefers_dark(theme: ThemeMode) -> bool {
    match theme {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => detect_system_dark(),
    }
}

/// 把原生标题栏设为深/浅（Win10 旧版退避属性 19；失败静默——非关键视觉）。
pub fn apply_titlebar_theme(window: &tauri::WebviewWindow, theme: ThemeMode) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let value = prefers_dark(theme) as i32;
    // SAFETY：本应用自有窗口句柄；仅设置标题栏深浅属性。
    for attr in [DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE(19)] {
        let hr = unsafe {
            DwmSetWindowAttribute(
                HWND(hwnd.0),
                attr,
                &value as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            )
        };
        if hr.is_ok() {
            return;
        }
    }
    log::debug!("ui_text: 标题栏主题设置失败（旧系统，忽略）");
}
