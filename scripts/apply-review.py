# -*- coding: utf-8 -*-
"""验收修复脚本：应用双盲审 + 自查的全部采纳项（一次性，可删）。"""
import io, re

def apply(path, reps, quote='"'):
    s = io.open(path, encoding='utf-8').read()
    missed = []
    for old, new in reps:
        if old in s:
            s = s.replace(old, new)
        else:
            missed.append(old[:56])
    io.open(path, 'w', encoding='utf-8', newline='\n').write(s)
    return missed

M = []

# ============ zh-TW：台湾本地化（托盤/識別/閾值/劃詞/快捷鍵 等） ============
M += [('zh-TW', apply('src/lib/i18n/zh-TW.ts', [
    ('與面板開關、托盤勾選三方即時同步', '與面板開關、系統匣勾選三方即時同步'),
    ('開啟後隨系統啟動常駐托盤', '開啟後隨系統啟動常駐系統匣'),
    ('Alya 將常駐系統托盤。', 'Alya 將常駐系統匣。'),
    ('識別結果', '辨識結果'),            # ocrResultTitle + aria 识别结果
    ('識別中', '辨識中'),
    ('未識別到文字', '未辨識到文字'),
    ('截圖識別', '截圖辨識'),
    ('截圖識別的文字擷取', '截圖辨識的文字擷取'),
    ('無法識別該語種', '無法辨識該語言'),
    ('固定語種需系統已裝語言套件', '固定語言需系統已裝語言套件'),
    ('長文整段閾值', '長文整段臨界值'),
    ('（出廠為 DeepSeek）', '（預設為 DeepSeek）'),
    ('介面位址', 'API 位址'),
    ('服務商模型標識', '服務商模型識別碼'),
    ("groupHotkeys: \"快速鍵\"", "groupHotkeys: \"快速鍵\"") if False else ('groupHotkeys: "快捷鍵"', 'groupHotkeys: "快速鍵"'),
    ('請按下新的快捷鍵', '請按下新的快速鍵'),
    ('該快捷鍵已被其他程式佔用，已復原原快捷鍵', '該快速鍵已被其他程式佔用，已復原原快速鍵'),
    ('常用快捷鍵：', '常用快速鍵：'),
    ('劃詞翻譯', '圈選翻譯'),
    ('Alt+Q 劃詞', 'Alt+Q 圈選'),
    ('輸入要翻譯的文本', '輸入要翻譯的文字'),
    ('正在翻譯長文本', '正在翻譯長文字'),
    ('本機加密儲存，任何檢視不回顯明文', '本機加密儲存，任何畫面皆不顯示明文'),
    ('請調整選區後重試', '請調整選取範圍後重試'),
    ('autostart: "開機自啟"', 'autostart: "開機自動啟動"'),
    ('暫時跳過，稍後在設定中配置', '暫時跳過，稍後再於設定中調整'),
]))]

# ============ ja ============
M += [('ja', apply('src/lib/i18n/ja.ts', [
    ('パネルとトレイとリアルタイム同期', 'パネル・トレイとリアルタイムに同期'),
    ('すべてストリーム', '常時ストリーミング'),
    ('了解しました、続ける', '確認して続ける'),
    ('autostart: "起動時に開始"', 'autostart: "自動起動"'),
    ('すべての入口にアクセスできます', 'すべての機能にアクセスできます'),
    ('空のまま保存すると解除', '空欄のまま保存すると削除されます'),
    ('システムの言語に従う', 'システムの優先言語に従う'),
    ('マイクロソフト公式ガイド', 'マイクロソフト公式ガイドを開く'),
    ("hide: \"隠す\"", "hide: \"非表示\""),
    ('後でもう一度お試しください。', 'しばらくしてから再試行してください。'),
    ('スクリーンショット', 'キャプチャ'),
    ('競合時は自動で元に戻します', '競合時は自動で元に戻して通知します'),
]))]

# ============ ko ============
M += [('ko', apply('src/lib/i18n/ko.ts', [
    ('기본값에 저장되지 않습니다', '기본값으로 저장되지 않습니다'),
    ('확인했습니다, 계속', '확인 후 계속'),
    ('현재 장면', '현재 모드'),
    ('장면 전환', '모드 전환'),
    ('클립보드 모니터"', '클립보드 모니터링"'),
    ('번역 보내기', '번역으로 보내기'),
    ('단계 모드에서', '단계 표시에서'),
    ('단계 모드에만 적용', '단계 표시에만 적용'),
    ('자동 번역이 꺼지면 무효', '자동 번역이 꺼져 있으면 적용되지 않음'),
    ('시스템 언어 따름', '시스템 선호 언어 따름'),
    ('(시스템 언어 따름)', '(시스템 선호 언어 따름)'),
    ('autostart: "시작 시 실행"', 'autostart: "시스템 시작 시 자동 실행"'),
    ('지금은 건너뛰고 나중에 설정에서 구성', '건너뛰고 나중에 설정'),
    ('충돌 시 자동 복귀', '충돌 시 자동 복귀 및 알림'),
    ('etaTail: "약 {n}초 남음"', 'etaTail: " 약 {n}초 남음"'),
]))]

# ============ fr：撇号 + 语义反转 + 术语 + eta ============
M += [('fr', apply('src/lib/i18n/fr.ts', [
    ("du texte nimporte où", "du texte n'importe où"),
    ("Le service na pas répondu", "Le service n'a pas répondu"),
    ("Compatible avec toute API OpenAI", "Fonctionne avec toute API compatible OpenAI"),
    ("Masquer sans focus", "Masquer à la perte du focus"),
    ("Texte par capture", "Capture de texte"),
    ("rester dans la barre des tâches", "rester dans la zone de notification"),
    ("depuis la barre des tâches", "depuis la zone de notification"),
    ("dans la barre des tâches", "dans la zone de notification"),
    ("la barre des tâches", "la zone de notification"),
    ("Réglez cela dans la console du fournisseur puis réessayez.", "Rechargez votre compte depuis la console du fournisseur puis réessayez."),
    ("laisser vide efface la clé", "laissez vide pour effacer la clé"),
    ("pour un saut de ligne", "pour aller à la ligne"),
    ('sceneMutual: "Mutuelle"', 'sceneMutual: "Bidirectionnelle"'),
    ('sceneDirected: "Dirigée"', 'sceneDirected: "Unidirectionnelle"'),
    ('etaStrong: "Est."', 'etaStrong: "Dans"'),
    ('etaTail: "environ {n} s restantes"', 'etaTail: " environ {n} s"'),
]))]

# fr 法式不换行空格（值内 " : " / " ; " → NBSP）
p = 'src/lib/i18n/fr.ts'
s = io.open(p, encoding='utf-8').read()
out = []
for line in s.splitlines():
    m = re.match(r'^(\s*\w+: )"(.*)",?$', line)
    if m:
        v = m.group(2).replace(' : ', '\u00A0: ').replace(' ; ', '\u00A0; ')
        line = f'{m.group(1)}"{v}",' + (',' if False else '')
        line = f'{m.group(1)}"{v}",'
        if not line.endswith(','):
            pass
    out.append(line)
io.open(p, 'w', encoding='utf-8', newline='\n').write('\n'.join(out) + '\n')

# ============ de ============
M += [('de', apply('src/lib/i18n/de.ts', [
    ('das ursprüngliche wurde wiederhergestellt', 'das ursprüngliche Kürzel wurde wiederhergestellt'),
    ('show: "Zeigen"', 'show: "Anzeigen"'),
    ('sendToTranslate: "Zum Übersetzen"', 'sendToTranslate: "Übersetzen"'),
    ('etaStrong: " ca."', 'etaStrong: "noch"'),
    ('etaTail: "etwa {n} s verbleibend"', 'etaTail: " ca. {n} s"'),
    ('Kurzbefehl per Tastendruck aufzeichnen; Konflikte werden zurückgesetzt', 'Tastenkürzel per Tastendruck aufzeichnen; bei Konflikten wird automatisch zurückgesetzt'),
    ('zeigt alle Einstiege', 'öffnet alle Funktionen'),
    ('Die Teilübersetzung wurde bewahrt.', 'Der bereits übersetzte Teil bleibt erhalten.'),
    ('Vorerst überspringen, später in den Einstellungen', 'Vorerst überspringen, später in den Einstellungen einrichten'),
    ('groupOcr: "Aufnahme"', 'groupOcr: "Screenshot"'),
    ('Alt+S/D Aufnahme', 'Alt+S/D Screenshot'),
    ('hotkeyOcrTranslate: "Aufnahme übersetzen"', 'hotkeyOcrTranslate: "Screenshot übersetzen"'),
    ('Aufnahme übersetzen Alt+D', 'Screenshot übersetzen Alt+D'),
]))]

# ============ es ============
M += [('es', apply('src/lib/i18n/es.ts', [
    ('Compatible con cualquier API de OpenAI', 'Funciona con cualquier API compatible con OpenAI'),
    ("title: 'Bienvenido a Alya'", "title: 'Te damos la bienvenida a Alya'") if False else ('title: "Bienvenido a Alya"', 'title: "Te damos la bienvenida a Alya"'),
    ('hotkeySelection: "Traducir selección"', 'hotkeySelection: "Traducción de selección"'),
    ('hotkeyInput: "Traducir texto escrito"', 'hotkeyInput: "Traducción de escritura"'),
    ('hotkeyToggleScene: "Cambiar escena"', 'hotkeyToggleScene: "Cambio de escena"'),
    ('presentationStream: "Todo en flujo"', 'presentationStream: "Todo con streaming"'),
    ('Omitir por ahora, configurar después', 'Omitir por ahora, configurar más tarde en Ajustes'),
    ('etaStrong: "Est."', 'etaStrong: "Quedan"'),
    ('etaTail: "faltan unos {n} s"', 'etaTail: " unos {n} s"'),
    ('sceneMutual: "Mutua"', 'sceneMutual: "Bidireccional"'),
    ('sceneDirected: "Dirigida"', 'sceneDirected: "Direccional"'),
]))]

# ============ ru ============
M += [('ru', apply('src/lib/i18n/ru.ts', [
    ('Запись новой комбинации; при конфликте откат автоматически', 'Запись нового сочетания; при конфликте прежнее сочетание восстанавливается автоматически с уведомлением'),
    ('открывает все входы', 'открывает доступ ко всем функциям'),
    ('Пропустить пока, настроить позже', 'Пропустить и настроить позже'),
    ('saveFailed: "Не сохранено: {msg}"', 'saveFailed: "Не удалось сохранить: {msg}"'),
    ('baseUrlDesc: "Совместимый с OpenAI протокол"', 'baseUrlDesc: "Протокол, совместимый с OpenAI"'),
    ('etaTail: "примерно {n} с"', 'etaTail: " примерно {n} с"'),
    ('sceneMutual: "Взаимный"', 'sceneMutual: "Взаимная"'),
    ('sceneDirected: "Направленный"', 'sceneDirected: "Направленная"'),
]))]

# ============ en：privacyNote 措辞 + eta 拼接空格 ============
M += [('en', apply('src/lib/i18n/en.ts', [
    ("capture recognition runs locally", "OCR runs locally"),
    ("etaStrong: 'Est.', etaTail: 'about {n} s left'" if False else "etaStrong: 'Est.'", "etaStrong: 'About'"),
    ("etaTail: 'about {n} s left'", "etaTail: ' {n} s left'"),
]))]

# ============ Rust ui_text：scene 标签（fr/es/ru）+ ko 托盤术语 ============
M += [('ui_text', apply('src-tauri/src/ui_text.rs', [
    ('Self::Fr => "Mutuelle"', 'Self::Fr => "Bidirectionnelle"'),
    ('Self::Fr => "Dirigée"', 'Self::Fr => "Unidirectionnelle"'),
    ('Self::Es => "Mutua"', 'Self::Es => "Bidireccional"'),
    ('Self::Es => "Dirigida"', 'Self::Es => "Direccional"'),
    ('Self::Ru => "Взаимный"', 'Self::Ru => "Взаимная"'),
    ('Self::Ru => "Направленный"', 'Self::Ru => "Направленная"'),
    ('Self::Ko => "클립보드 모니터"', 'Self::Ko => "클립보드 모니터링"'),
]))]

for name, missed in M:
    print(f'{name}: {"OK" if not missed else "MISSED " + str(missed)}')
