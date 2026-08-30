# -*- coding: utf-8 -*-
"""i18n-3 接线脚本（一次性，可删）：错误消息目录接入 hotkey/settings/autostart/前端。"""
import io

def apply(path, reps):
    s = io.open(path, encoding='utf-8').read()
    missed = []
    for old, new in reps:
        if old in s:
            s = s.replace(old, new)
        else:
            missed.append(old[:60])
    io.open(path, 'w', encoding='utf-8', newline='\n').write(s)
    return missed

# 1) lib.rs 注册模块
m = apply('src-tauri/src/lib.rs', [
    ("pub mod error;", "pub mod error;\npub mod error_text;"),
])
print('lib.rs missed:', m)

# 2) hotkey/mod.rs
m = apply('src-tauri/src/hotkey/mod.rs', [
    ("""pub fn rebind_hotkey(
    app: AppHandle,
    function: HotkeyFunction,
    combo: String,
) -> Result<(), AppError> {
    if combo.trim().is_empty() {
        return Err(conflict_error(
            function,
            &combo,
            "快捷键不能为空（至少一个修饰键 + 主键）",
        ));""",
     """pub fn rebind_hotkey(
    app: AppHandle,
    function: HotkeyFunction,
    combo: String,
) -> Result<(), AppError> {
    // i18n-3：错误消息按当前界面语言解析（config general.language）。
    let lang = crate::ui_text::resolve_global();
    if combo.trim().is_empty() {
        return Err(conflict_error(
            lang,
            function,
            &combo,
            lang.hotkey_empty_reason(),
        ));"""),
    ("""            &format!(
                "已被「{}」占用，请先解除或另选组合",
                function_display_name(owner)
            ),""",
     """            &lang.hotkey_taken_reason(function_display_name(owner, lang)),"""),
    ("""            return Err(AppError::Config(format!(
                "hotkey: {function:?} 未处于注册态（启动注册失败），请先改键恢复"
            )))""",
     """            return Err(AppError::Config(lang.hotkey_not_registered(&format!(
                "{function:?}"
            ))))"""),
    ("""            AppError::HotkeyConflict(format!(
                "解注旧快捷键 {old_combo} 失败：{err}（状态未变更）"
            ))?;""",
     """            AppError::HotkeyConflict(
                lang.hotkey_unregister_failed(&old_combo, &err.to_string()),
            )?;"""),
    ("""            let err = conflict_error(
                function,
                &combo,
                "注册失败（可能已被系统或其他程序占用），已回退为原快捷键",
            );""",
     """            let err = conflict_error(
                lang,
                function,
                &combo,
                lang.hotkey_register_fail_rollback(),
            );"""),
    ("""fn conflict_error(function: HotkeyFunction, combo: &str, reason: &str) -> AppError {
    AppError::HotkeyConflict(format!(
        "快捷键「{combo}」绑定失败：{}（{}）",
        function_display_name(function),
        reason
    ))
}""",
     """fn conflict_error(
    lang: crate::config::Language,
    function: HotkeyFunction,
    combo: &str,
    reason: &str,
) -> AppError {
    AppError::HotkeyConflict(lang.hotkey_conflict_wrapper(
        combo,
        &function_display_name(function, lang),
        reason,
    ))
}"""),
    ("""pub fn function_display_name(function: HotkeyFunction) -> &'static str {
    match function {
        HotkeyFunction::Selection => "划词翻译",
        HotkeyFunction::Input => "输入翻译",
        HotkeyFunction::OcrRecognize => "截图识别",
        HotkeyFunction::OcrTranslate => "截图翻译",
        HotkeyFunction::ToggleScene => "切换场景",
    }
}""",
     """/// 功能显示名（i18n-3：按界面语言；目录在 error_text）。
pub fn function_display_name(
    function: HotkeyFunction,
    lang: crate::config::Language,
) -> &'static str {
    match function {
        HotkeyFunction::Selection => lang.func_selection(),
        HotkeyFunction::Input => lang.func_input(),
        HotkeyFunction::OcrRecognize => lang.func_ocr_recognize(),
        HotkeyFunction::OcrTranslate => lang.func_ocr_translate(),
        HotkeyFunction::ToggleScene => lang.func_toggle_scene(),
    }
}"""),
    ("""fn notify_registration_failures(app: &AppHandle, failed: &[HotkeyFunction]) {
    let names = failed
        .iter()
        .map(|function| function_display_name(*function))
        .collect::<Vec<_>>()
        .join("、");
    let mut body = format!(
        "以下功能的快捷键注册失败（可能已被系统或其他程序占用），可在设置中重新指定后立即恢复：{names}。"
    );
    if failed.contains(&HotkeyFunction::Selection) {
        body.push_str("划词翻译需改键后方可触发；其余功能仍可经托盘菜单使用（FR-14.3）。");
    }
    notify_hotkey_error(app, body);
}""",
     """fn notify_registration_failures(app: &AppHandle, failed: &[HotkeyFunction]) {
    let lang = crate::ui_text::resolve_global();
    let names = failed
        .iter()
        .map(|function| function_display_name(*function, lang))
        .collect::<Vec<_>>()
        .join(lang.list_separator());
    let mut body = lang.hotkey_startup_fail_notify(&names);
    if failed.contains(&HotkeyFunction::Selection) {
        body.push_str(&lang.hotkey_selection_hint());
    }
    notify_hotkey_error(app, body);
}"""),
    # 测试适配（语言钉死 zh，保证断言确定性）
    ('let err = conflict_error(HotkeyFunction::Selection, "Ctrl+D", "已被「截图识别」占用");',
     'let err = conflict_error(\n            crate::config::Language::ZhCn,\n            HotkeyFunction::Selection,\n            "Ctrl+D",\n            "已被「截图识别」占用",\n        );'),
    ('assert_eq!(function_display_name(HotkeyFunction::Selection), "划词翻译");',
     'assert_eq!(function_display_name(HotkeyFunction::Selection, crate::config::Language::ZhCn), "划词翻译");'),
    ('assert_eq!(function_display_name(HotkeyFunction::Input), "输入翻译");',
     'assert_eq!(function_display_name(HotkeyFunction::Input, crate::config::Language::ZhCn), "输入翻译");'),
    ('function_display_name(HotkeyFunction::OcrRecognize),',
     'function_display_name(HotkeyFunction::OcrRecognize, crate::config::Language::ZhCn),'),
    ('function_display_name(HotkeyFunction::OcrTranslate),',
     'function_display_name(HotkeyFunction::OcrTranslate, crate::config::Language::ZhCn),'),
    ('function_display_name(HotkeyFunction::ToggleScene),',
     'function_display_name(HotkeyFunction::ToggleScene, crate::config::Language::ZhCn),'),
])
print('hotkey missed:', m)

# 3) settings.rs 专通道拒绝
m = apply('src-tauri/src/windows/settings.rs', [
    ('            "热键变更不走 set_settings：请用 rebind_hotkey 专用命令（spec 6.1）".to_string(),',
     '            crate::ui_text::resolve_global().settings_reject_hotkey_channel(),'),
    ('''            "剪贴板监听开关不走 set_settings：请用 set_clipboard_monitor 三方统一入口（spec 6.1）"
                .to_string(),''',
     '            crate::ui_text::resolve_global().settings_reject_clipboard_channel(),'),
])
print('settings missed:', m)

# 4) autostart.rs
m = apply('src-tauri/src/autostart.rs', [
    ('.map_err(|err| AppError::Config(format!("开机自启状态查询失败: {err}")))?;',
     '.map_err(|err| AppError::Config(crate::ui_text::resolve_global().autostart_query_failed(&err.to_string())))?;'),
    ('.map_err(|err| AppError::Config(format!("开机自启写入失败: {err}")))?,',
     '.map_err(|err| AppError::Config(crate::ui_text::resolve_global().autostart_write_failed(&err.to_string())))?,'),
    ('.map_err(|err| AppError::Config(format!("开机自启移除失败: {err}")))?,',
     '.map_err(|err| AppError::Config(crate::ui_text::resolve_global().autostart_remove_failed(&err.to_string())))?,'),
])
print('autostart missed:', m)

# 5) 前端 rebindRow：原样呈现本地化消息
m = apply('src/settings/App.svelte', [
    ("""      const msg = String(err);
      // 内部共键冲突（后端点名占用方）→ 用后端消息；外部占用/注册失败 → design-spec 固定文案
      rowErrors[key] = msg.includes('已被「') ? msg : conflictRestored();""",
     "      rowErrors[key] = String(err); // i18n-3：后端已按界面语言本地化，原样呈现"),
    ("""  /** design-spec 7.2 冲突/注册失败回退文案（外部占用；内部共键冲突显示后端点名消息）。 */
  const conflictRestored = (): string => t('settings.hotkeyConflictRestored');

""", ""),
])
print('frontend missed:', m)

# 6) 冲突回退测试断言更新
m = apply('src/settings/App.test.ts', [
    ("expect(alert.textContent).toContain('该快捷键已被其他程序占用，已恢复原快捷键');",
     "expect(alert.textContent).toContain('已回退为原快捷键'); // i18n-3：原样呈现后端本地化消息"),
])
print('test missed:', m)
