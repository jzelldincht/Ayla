//! 错误消息目录（i18n-3）：热键改键 / 设置专通道拒绝 / 开机自启——到达设置页行内 error
//! 与系统通知的用户可见文本。与 `ui_text` 同为壳层文案目录；分文件仅按职责切分，
//! `impl Language` 跨模块固有实现合法（同 crate）。前端不再做中文启发式判断，原样呈现。

use crate::config::Language;

impl Language {
    /// 列表连接符（中文顿号，其余逗号）。
    pub fn list_separator(self) -> &'static str {
        match self {
            Self::System | Self::ZhCn | Self::ZhTw => "、",
            _ => ", ",
        }
    }

    pub fn func_selection(self) -> &'static str {
        match self {
            Self::System | Self::ZhCn => "划词翻译",
            Self::ZhTw => "劃詞翻譯",
            Self::En => "Selection translate",
            Self::Ja => "選択翻訳",
            Self::Ko => "선택 번역",
            Self::Fr => "Traduction par sélection",
            Self::De => "Auswahl übersetzen",
            Self::Es => "Traducir selección",
            Self::Ru => "Перевод выделения",
        }
    }

    pub fn func_input(self) -> &'static str {
        match self {
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

    pub fn func_ocr_recognize(self) -> &'static str {
        match self {
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

    pub fn func_ocr_translate(self) -> &'static str {
        match self {
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

    pub fn func_toggle_scene(self) -> &'static str {
        match self {
            Self::System | Self::ZhCn => "切换场景",
            Self::ZhTw => "切換場景",
            Self::En => "Toggle scene",
            Self::Ja => "シーン切替",
            Self::Ko => "장면 전환",
            Self::Fr => "Basculer la scène",
            Self::De => "Szene wechseln",
            Self::Es => "Cambiar escena",
            Self::Ru => "Переключить сцену",
        }
    }

    pub fn hotkey_empty_reason(self) -> &'static str {
        match self {
            Self::System | Self::ZhCn => "快捷键不能为空（至少一个修饰键 + 主键）",
            Self::ZhTw => "快捷鍵不能為空（至少一個修飾鍵 + 主鍵）",
            Self::En => "The shortcut cannot be empty (at least one modifier + a key)",
            Self::Ja => "ショートカットを空にできません（修飾キー + キーが必要）",
            Self::Ko => "단축키는 비울 수 없습니다(수정 키 + 키가 필요합니다)",
            Self::Fr => "Le raccourci ne peut pas être vide (au moins un modificateur + une touche)",
            Self::De => "Das Tastenkürzel darf nicht leer sein (mindestens ein Modifikator + eine Taste)",
            Self::Es => "El atajo no puede estar vacío (un modificador + una tecla, como mínimo)",
            Self::Ru => "Сочетание не может быть пустым (минимум модификатор + клавиша)",
        }
    }

    pub fn hotkey_taken_reason(self, name: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("已被「{name}」占用，请先解除或另选组合"),
            Self::ZhTw => format!("已被「{name}」佔用，請先解除或另選組合"),
            Self::En => format!("Already taken by \"{name}\"; remove it there or pick another combination"),
            Self::Ja => format!("「{name}」が使用中です。解除するか別の組み合わせを選んでください"),
            Self::Ko => format!("\"{name}\"이(가) 사용 중입니다. 해제하거나 다른 조합을 선택하세요"),
            Self::Fr => format!("Déjà utilisé par « {name} » ; libérez-le ou choisissez une autre combinaison"),
            Self::De => format!("Bereits von \"{name}\" belegt; bitte dort lösen oder eine andere Kombination wählen"),
            Self::Es => format!("Ya lo usa \"{name}\"; libéralo o elige otra combinación"),
            Self::Ru => format!("Уже занято «{name}»; освободите его или выберите другое сочетание"),
        }
    }

    pub fn hotkey_register_fail_rollback(self) -> &'static str {
        match self {
            Self::System | Self::ZhCn => "注册失败（可能已被系统或其他程序占用），已回退为原快捷键",
            Self::ZhTw => "註冊失敗（可能已被系統或其他程式佔用），已回復為原快捷鍵",
            Self::En => "Registration failed (it may be taken by the system or another app); reverted to the original shortcut",
            Self::Ja => "登録に失敗しました（システムまたは他のアプリで使用中の可能性）。元のショートカットに戻しました",
            Self::Ko => "등록 실패(시스템이나 다른 앱이 사용 중일 수 있습니다). 원래 단축키로 복원했습니다",
            Self::Fr => "Échec de l'enregistrement (peut-être utilisé par le système ou une autre application) ; retour à l'ancien raccourci",
            Self::De => "Registrierung fehlgeschlagen (möglicherweise vom System oder einer anderen App belegt); auf das ursprüngliche Kürzel zurückgesetzt",
            Self::Es => "No se pudo registrar (puede que lo use el sistema u otra aplicación); se restauró el atajo original",
            Self::Ru => "Не удалось зарегистрировать (возможно, занято системой или другой программой); возвращено прежнее сочетание",
        }
    }

    pub fn hotkey_conflict_wrapper(self, combo: &str, func: &str, reason: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("快捷键「{combo}」绑定失败：{func}（{reason}）"),
            Self::ZhTw => format!("快捷鍵「{combo}」綁定失敗：{func}（{reason}）"),
            Self::En => format!("Failed to bind \"{combo}\" for {func}: ({reason})"),
            Self::Ja => format!("ショートカット「{combo}」の割り当てに失敗：{func}（{reason}）"),
            Self::Ko => format!("\"{combo}\" 단축키 지정 실패: {func}({reason})"),
            Self::Fr => format!("Échec d'attribution de « {combo} » pour {func} : ({reason})"),
            Self::De => format!("\"{combo}\" konnte für {func} nicht gebunden werden: ({reason})"),
            Self::Es => format!("No se pudo asignar \"{combo}\" a {func}: ({reason})"),
            Self::Ru => format!("Не удалось привязать «{combo}» для {func}: ({reason})"),
        }
    }

    pub fn hotkey_not_registered(self, func_dbg: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("该功能（{func_dbg}）当前未注册（启动注册失败），请先改键恢复"),
            Self::ZhTw => format!("該功能（{func_dbg}）目前未註冊（啟動註冊失敗），請先改鍵恢復"),
            Self::En => format!("This function ({func_dbg}) is not registered (startup registration failed); rebind it in Settings first"),
            Self::Ja => format!("この機能（{func_dbg}）は未登録です（起動時の登録に失敗）。設定で再設定してください"),
            Self::Ko => format!("이 기능({func_dbg})은 등록되지 않았습니다(시작 등록 실패). 설정에서 다시 지정하세요"),
            Self::Fr => format!("Cette fonction ({func_dbg}) n'est pas enregistrée (échec au démarrage) ; reconfigurez-la dans les paramètres"),
            Self::De => format!("Diese Funktion ({func_dbg}) ist nicht registriert (Startregistrierung fehlgeschlagen); bitte in den Einstellungen neu zuweisen"),
            Self::Es => format!("Esta función ({func_dbg}) no está registrada (falló el registro inicial); reconfigúrala en Ajustes"),
            Self::Ru => format!("Эта функция ({func_dbg}) не зарегистрирована (сбой при запуске); переназначьте её в настройках"),
        }
    }

    pub fn hotkey_unregister_failed(self, combo: &str, err: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("解注旧快捷键 {combo} 失败：{err}（状态未变更）"),
            Self::ZhTw => format!("解除舊快捷鍵 {combo} 失敗：{err}（狀態未變更）"),
            Self::En => format!("Failed to unregister the old shortcut {combo}: {err} (nothing changed)"),
            Self::Ja => format!("旧ショートカット {combo} の解除に失敗：{err}（状態は変更されていません）"),
            Self::Ko => format!("기존 단축키 {combo} 해제 실패: {err}(상태 변경 없음)"),
            Self::Fr => format!("Échec de la libération de l'ancien raccourci {combo} : {err} (aucun changement)"),
            Self::De => format!("Das alte Kürzel {combo} konnte nicht freigegeben werden: {err} (nichts geändert)"),
            Self::Es => format!("No se pudo liberar el atajo anterior {combo}: {err} (sin cambios)"),
            Self::Ru => format!("Не удалось освободить прежнее сочетание {combo}: {err} (состояние не изменено)"),
        }
    }

    pub fn hotkey_startup_fail_notify(self, names: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("以下功能的快捷键注册失败（可能已被系统或其他程序占用），可在设置中重新指定后立即恢复：{names}。"),
            Self::ZhTw => format!("以下功能的快捷鍵註冊失敗（可能已被系統或其他程式佔用），可在設定中重新指定後立即恢復：{names}。"),
            Self::En => format!("These shortcuts failed to register (they may be taken by the system or other apps): {names}. Rebind them in Settings to restore."),
            Self::Ja => format!("次のショートカットの登録に失敗しました（システムまたは他のアプリで使用中の可能性があります）：{names}。設定で再設定すると復旧します。"),
            Self::Ko => format!("다음 단축키 등록에 실패했습니다(시스템이나 다른 앱이 사용 중일 수 있음): {names}. 설정에서 다시 지정하면 복구됩니다."),
            Self::Fr => format!("Ces raccourcis n'ont pas pu être enregistrés (peut-être utilisés par le système ou d'autres applications) : {names}. Reconfigurez-les dans les paramètres pour les restaurer."),
            Self::De => format!("Folgende Tastenkürzel konnten nicht registriert werden (möglicherweise belegt): {names}. In den Einstellungen neu zuweisen, um sie wiederherzustellen."),
            Self::Es => format!("Estos atajos no se pudieron registrar (pueden estar en uso por el sistema u otras aplicaciones): {names}. Reconfigúralos en Ajustes para restaurarlos."),
            Self::Ru => format!("Не удалось зарегистрировать следующие сочетания (возможно, заняты): {names}. Переназначьте их в настройках для восстановления."),
        }
    }

    pub fn hotkey_selection_hint(self) -> String {
        match self {
            Self::System | Self::ZhCn => "划词翻译需改键后方可触发；其余功能仍可经托盘菜单使用。".to_string(),
            Self::ZhTw => "劃詞翻譯需改鍵後方可觸發；其餘功能仍可經托盤選單使用。".to_string(),
            Self::En => "Selection translate needs a rebind before it works; the other functions remain available from the tray menu.".to_string(),
            Self::Ja => "選択翻訳は再設定するまで動作しません。他の機能はトレイメニューから利用できます。".to_string(),
            Self::Ko => "선택 번역은 다시 지정해야 동작합니다. 다른 기능은 트레이 메뉴에서 계속 사용할 수 있습니다.".to_string(),
            Self::Fr => "La traduction par sélection nécessite une reconfiguration ; les autres fonctions restent disponibles depuis la barre des tâches.".to_string(),
            Self::De => "Auswahl übersetzen funktioniert erst nach einer Neuzuweisung; die anderen Funktionen bleiben über das Tray-Menü verfügbar.".to_string(),
            Self::Es => "Traducir selección necesita reconfigurarse para funcionar; las demás funciones siguen disponibles en la bandeja.".to_string(),
            Self::Ru => "Перевод выделения заработает после переназначения; остальные функции доступны через меню трея.".to_string(),
        }
    }

    pub fn settings_reject_hotkey_channel(self) -> String {
        match self {
            Self::System | Self::ZhCn => "热键变更不走 set_settings：请使用 rebind_hotkey 专用命令。".to_string(),
            Self::ZhTw => "熱鍵變更不走 set_settings：請使用 rebind_hotkey 專用命令。".to_string(),
            Self::En => "Hotkey changes do not go through set_settings; use the dedicated rebind_hotkey command.".to_string(),
            Self::Ja => "ショートカット変更は set_settings を経由しません。専用の rebind_hotkey を使用してください。".to_string(),
            Self::Ko => "단축키 변경은 set_settings를 거치지 않습니다. 전용 rebind_hotkey를 사용하세요.".to_string(),
            Self::Fr => "Les changements de raccourcis ne passent pas par set_settings ; utilisez la commande dédiée rebind_hotkey.".to_string(),
            Self::De => "Tastenkürzel-Änderungen laufen nicht über set_settings; bitte den eigenen Befehl rebind_hotkey verwenden.".to_string(),
            Self::Es => "Los cambios de atajos no van por set_settings; usa el comando dedicado rebind_hotkey.".to_string(),
            Self::Ru => "Изменение горячих клавиш не проходит через set_settings; используйте отдельную команду rebind_hotkey.".to_string(),
        }
    }

    pub fn settings_reject_clipboard_channel(self) -> String {
        match self {
            Self::System | Self::ZhCn => "剪贴板监听开关不走 set_settings：请使用 set_clipboard_monitor 统一入口。".to_string(),
            Self::ZhTw => "剪貼簿監聽開關不走 set_settings：請使用 set_clipboard_monitor 統一入口。".to_string(),
            Self::En => "The clipboard monitor toggle does not go through set_settings; use the set_clipboard_monitor command.".to_string(),
            Self::Ja => "クリップボード監視の切替は set_settings を経由しません。set_clipboard_monitor を使用してください。".to_string(),
            Self::Ko => "클립보드 모니터 토글은 set_settings를 거치지 않습니다. set_clipboard_monitor를 사용하세요.".to_string(),
            Self::Fr => "La surveillance du presse-papiers ne passe pas par set_settings ; utilisez la commande set_clipboard_monitor.".to_string(),
            Self::De => "Die Zwischenablage-Überwachung läuft nicht über set_settings; bitte set_clipboard_monitor verwenden.".to_string(),
            Self::Es => "El monitor del portapapeles no va por set_settings; usa el comando set_clipboard_monitor.".to_string(),
            Self::Ru => "Монитор буфера обмена не переключается через set_settings; используйте команду set_clipboard_monitor.".to_string(),
        }
    }

    pub fn autostart_query_failed(self, err: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("开机自启状态查询失败: {err}"),
            Self::ZhTw => format!("開機自啟狀態查詢失敗: {err}"),
            Self::En => format!("Failed to query the autostart state: {err}"),
            Self::Ja => format!("自動起動の状態取得に失敗: {err}"),
            Self::Ko => format!("자동 시작 상태 조회 실패: {err}"),
            Self::Fr => format!("Échec de la consultation de l'état de démarrage automatique : {err}"),
            Self::De => format!("Autostart-Status konnte nicht abgefragt werden: {err}"),
            Self::Es => format!("No se pudo consultar el estado de inicio automático: {err}"),
            Self::Ru => format!("Не удалось получить состояние автозапуска: {err}"),
        }
    }

    pub fn autostart_write_failed(self, err: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("开机自启写入失败: {err}"),
            Self::ZhTw => format!("開機自啟寫入失敗: {err}"),
            Self::En => format!("Failed to enable autostart: {err}"),
            Self::Ja => format!("自動起動の有効化に失敗: {err}"),
            Self::Ko => format!("자동 시작 활성화 실패: {err}"),
            Self::Fr => format!("Échec de l'activation du démarrage automatique : {err}"),
            Self::De => format!("Autostart konnte nicht aktiviert werden: {err}"),
            Self::Es => format!("No se pudo activar el inicio automático: {err}"),
            Self::Ru => format!("Не удалось включить автозапуск: {err}"),
        }
    }

    pub fn autostart_remove_failed(self, err: &str) -> String {
        match self {
            Self::System | Self::ZhCn => format!("开机自启移除失败: {err}"),
            Self::ZhTw => format!("開機自啟移除失敗: {err}"),
            Self::En => format!("Failed to disable autostart: {err}"),
            Self::Ja => format!("自動起動の無効化に失敗: {err}"),
            Self::Ko => format!("자동 시작 비활성화 실패: {err}"),
            Self::Fr => format!("Échec de la désactivation du démarrage automatique : {err}"),
            Self::De => format!("Autostart konnte nicht deaktiviert werden: {err}"),
            Self::Es => format!("No se pudo desactivar el inicio automático: {err}"),
            Self::Ru => format!("Не удалось отключить автозапуск: {err}"),
        }
    }

    // 包身份（MSIX/Store）StartupTask 专用（i18n-3 扩展：autostart 双模式）
    pub fn autostart_task_missing(self) -> String {
        match self {
            Self::System | Self::ZhCn => "未找到包清单声明的启动任务（安装包可能不完整）。".to_string(),
            Self::ZhTw => "未找到套件清單宣告的啟動工作（安裝套件可能不完整）。".to_string(),
            Self::En => "The startup task declared in the package manifest was not found (the package may be incomplete).".to_string(),
            Self::Ja => "パッケージ マニフェストに宣言されたスタートアップ タスクが見つかりません（パッケージが不完全な可能性）。".to_string(),
            Self::Ko => "패키지 매니페스트에 선언된 시작 작업을 찾을 수 없습니다(패키지가 불완전할 수 있음).".to_string(),
            Self::Fr => "La tâche de démarrage déclarée dans le manifeste du package est introuvable (package peut-être incomplet).".to_string(),
            Self::De => "Die im Paketmanifest deklarierte Startaufgabe wurde nicht gefunden (Paket ggf. unvollständig).".to_string(),
            Self::Es => "No se encontró la tarea de inicio declarada en el manifiesto del paquete (el paquete puede estar incompleto).".to_string(),
            Self::Ru => "Задача запуска, объявленная в манифесте пакета, не найдена (возможно, пакет неполон).".to_string(),
        }
    }
    pub fn autostart_user_disabled(self) -> String {
        match self {
            Self::System | Self::ZhCn => "开机自启此前已在 Windows 设置中被你关闭；请到 设置 > 应用 > 启动 重新开启。".to_string(),
            Self::ZhTw => "開機自動啟動先前已在 Windows 設定中被你關閉；請到 設定 > 應用程式 > 啟動 重新開啟。".to_string(),
            Self::En => "Autostart was turned off by you in Windows Settings; re-enable it under Settings > Apps > Startup.".to_string(),
            Self::Ja => "自動起動は Windows の設定でオフにされています。[設定] > [アプリ] > [スタートアップ] で再度オンにしてください。".to_string(),
            Self::Ko => "자동 시작이 Windows 설정에서 꺼져 있습니다. 설정 > 앱 > 시작에서 다시 켜세요.".to_string(),
            Self::Fr => "Le démarrage automatique a été désactivé dans les paramètres Windows ; réactivez-le sous Paramètres > Applications > Démarrage.".to_string(),
            Self::De => "Der Autostart wurde in den Windows-Einstellungen deaktiviert; reaktivieren Sie ihn unter Einstellungen > Apps > Start.".to_string(),
            Self::Es => "El inicio automático se desactivó en la configuración de Windows; reactívalo en Configuración > Aplicaciones > Inicio.".to_string(),
            Self::Ru => "Автозапуск был отключён вами в параметрах Windows; включите его снова в Параметры > Приложения > Автозагрузка.".to_string(),
        }
    }
    pub fn autostart_policy_disabled(self) -> String {
        match self {
            Self::System | Self::ZhCn => "开机自启已被系统策略禁用（组织管理设备）。".to_string(),
            Self::ZhTw => "開機自動啟動已被系統原則停用（組織管理裝置）。".to_string(),
            Self::En => "Autostart has been disabled by system policy (organization-managed device).".to_string(),
            Self::Ja => "自動起動はシステム ポリシーにより無効化されています（組織管理デバイス）。".to_string(),
            Self::Ko => "자동 시작이 시스템 정책에 의해 비활성화되었습니다(조직 관리 디바이스).".to_string(),
            Self::Fr => "Le démarrage automatique a été désactivé par une stratégie système (appareil géré par une organisation).".to_string(),
            Self::De => "Der Autostart wurde durch eine Systemrichtlinie deaktiviert (organisationsverwaltetes Gerät).".to_string(),
            Self::Es => "El inicio automático está deshabilitado por directiva del sistema (dispositivo administrado por una organización).".to_string(),
            Self::Ru => "Автозапуск отключён системной политикой (управляемое организацией устройство).".to_string(),
        }
    }
    pub fn autostart_enable_rejected(self) -> String {
        match self {
            Self::System | Self::ZhCn => "开机自启启用未生效（请求未被系统接受）。".to_string(),
            Self::ZhTw => "開機自動啟動啟用未生效（請求未被系統接受）。".to_string(),
            Self::En => "Enabling autostart did not take effect (the request was not accepted by the system).".to_string(),
            Self::Ja => "自動起動の有効化が反映されませんでした（システムに受け付けられませんでした）。".to_string(),
            Self::Ko => "자동 시작 활성화가 적용되지 않았습니다(시스템이 요청을 수락하지 않음).".to_string(),
            Self::Fr => "L'activation du démarrage automatique n'a pas pris effet (la demande n'a pas été acceptée par le système).".to_string(),
            Self::De => "Das Aktivieren des Autostarts hatte keine Wirkung (die Anfrage wurde vom System nicht angenommen).".to_string(),
            Self::Es => "La activación del inicio automático no surtió efecto (el sistema no aceptó la solicitud).".to_string(),
            Self::Ru => "Включение автозапуска не вступило в силу (система не приняла запрос).".to_string(),
        }
    }
}
