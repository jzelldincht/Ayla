# -*- coding: utf-8 -*-
"""UI优化清单 i18n 扩充脚本（一次性，可删）：
langs 目标语言名矩阵（P1-2）/ mask.dragHint 新分区（P1-4）/ onb.keyPlaceholder（P3-10）/
settings.restoreDefaults(+Confirm)（P3-11）/ panel.clipboardToggleHint 短句化（P2-5）。
分区感知：各键写入所属分区，全部九文件结构一致（typeof zh 互检）。"""
import io, json, re

LOCALES = ['zh-CN', 'zh-TW', 'en', 'ja', 'ko', 'fr', 'de', 'es', 'ru']

LANGS = {
 'zh-CN': ['中文(简体)', '中文(繁体)', '英文', '日文', '韩文', '法文', '德文', '西班牙文', '俄文'],
 'zh-TW': ['中文(簡體)', '中文(繁體)', '英文', '日文', '韓文', '法文', '德文', '西班牙文', '俄文'],
 'en': ['Chinese (Simplified)', 'Chinese (Traditional)', 'English', 'Japanese', 'Korean', 'French', 'German', 'Spanish', 'Russian'],
 'ja': ['中国語（簡体字）', '中国語（繁体字）', '英語', '日本語', '韓国語', 'フランス語', 'ドイツ語', 'スペイン語', 'ロシア語'],
 'ko': ['중국어(간체)', '중국어(번체)', '영어', '일본어', '한국어', '프랑스어', '독일어', '스페인어', '러시아어'],
 'fr': ['Chinois (simplifié)', 'Chinois (traditionnel)', 'Anglais', 'Japonais', 'Coréen', 'Français', 'Allemand', 'Espagnol', 'Russe'],
 'de': ['Chinesisch (vereinfacht)', 'Chinesisch (traditionell)', 'Englisch', 'Japanisch', 'Koreanisch', 'Französisch', 'Deutsch', 'Spanisch', 'Russisch'],
 'es': ['Chino (simplificado)', 'Chino (tradicional)', 'Inglés', 'Japonés', 'Coreano', 'Francés', 'Alemán', 'Español', 'Ruso'],
 'ru': ['Китайский (упрощённый)', 'Китайский (традиционный)', 'Английский', 'Японский', 'Корейский', 'Французский', 'Немецкий', 'Испанский', 'Русский'],
}
LANG_KEYS = ['targetZhHans', 'targetZhHant', 'targetEn', 'targetJa', 'targetKo', 'targetFr', 'targetDe', 'targetEs', 'targetRu']

HINT = {
 'zh-CN': '自动翻译新复制的内容', 'zh-TW': '自動翻譯新複製的內容',
 'en': 'Auto-translate new clipboard content', 'ja': 'コピーした内容を自動翻訳',
 'ko': '새로 복사한 내용 자동 번역', 'fr': 'Traduire automatiquement le contenu copié',
 'de': 'Neu kopierten Inhalt automatisch übersetzen', 'es': 'Traducir automáticamente lo copiado',
 'ru': 'Автоперевод нового скопированного текста',
}
DRAG = {
 'zh-CN': '拖拽选择区域 · Esc 取消', 'zh-TW': '拖曳選取範圍 · Esc 取消',
 'en': 'Drag to select an area · Esc to cancel', 'ja': 'ドラッグで範囲選択 · Esc でキャンセル',
 'ko': '드래그로 영역 선택 · Esc로 취소', 'fr': 'Glissez pour sélectionner · Échap pour annuler',
 'de': 'Ziehen, um einen Bereich auszuwählen · Esc bricht ab', 'es': 'Arrastra para seleccionar · Esc para cancelar',
 'ru': 'Выделите область мышью · Esc — отмена',
}
KEY_PH = {
 'zh-CN': '粘贴 API Key', 'zh-TW': '貼上 API Key', 'en': 'Paste API key', 'ja': 'APIキーを貼り付け',
 'ko': 'API 키 붙여넣기', 'fr': 'Collez la clé API', 'de': 'API-Schlüssel einfügen',
 'es': 'Pega la clave de API', 'ru': 'Вставьте API-ключ',
}
RESTORE = {
 'zh-CN': '恢复默认设置', 'zh-TW': '還原預設設定', 'en': 'Restore defaults', 'ja': '設定をリセット',
 'ko': '기본 설정 복원', 'fr': 'Rétablir les valeurs par défaut', 'de': 'Standardwerte wiederherstellen',
 'es': 'Restaurar valores predeterminados', 'ru': 'Сбросить настройки',
}
RESTORE_CONFIRM = {
 'zh-CN': '确定恢复全部设置为出厂默认值吗？（API Key、界面语言与引导状态将保留）',
 'zh-TW': '確定還原全部設定為出廠預設值嗎？（API Key、介面語言與引導狀態將保留）',
 'en': 'Reset all settings to factory defaults? (API key, interface language and onboarding state will be kept.)',
 'ja': 'すべての設定を初期値に戻しますか？（API キー・表示言語・初期設定状態は保持されます）',
 'ko': '모든 설정을 출고값으로 되돌릴까요?(API 키, 인터페이스 언어, 온보딩 상태는 유지)',
 "fr": "Réinitialiser tous les paramètres aux valeurs d'usine ? (La clé API, la langue et l'état de bienvenue seront conservés.)",
 'de': 'Alle Einstellungen auf Werkseinstellungen zurücksetzen? (API-Schlüssel, Sprache und Onboarding-Status bleiben erhalten.)',
 'es': '¿Restablecer todos los ajustes a sus valores de fábrica? (Se conservan la clave de API, el idioma y el estado de bienvenida.)',
 'ru': 'Сбросить все настройки к заводским? (API-ключ, язык интерфейса и состояние первичной настройки будут сохранены.)',
}

for loc in LOCALES:
    path = f'src/lib/i18n/{loc}.ts'
    s = io.open(path, encoding='utf-8').read()

    # 1) panel.clipboardToggleHint 值替换（panel 分区现有键）
    s = re.sub(r'(clipboardToggleHint: ")[^"]*(")',
               lambda m: m.group(1) + HINT[loc] + m.group(2), s, count=1)

    # 2) onb.keyPlaceholder（onb 分区 fieldApiKey 行后插入）
    s = re.sub(r'(    fieldApiKey: .*\n)',
               lambda m: m.group(1) + '    keyPlaceholder: ' + json.dumps(KEY_PH[loc], ensure_ascii=False) + ',\n',
               s, count=1)

    # 3) settings.restoreDefaults(+Confirm)（settings 分区 langRu 行后插入）
    ins = ('    restoreDefaults: ' + json.dumps(RESTORE[loc], ensure_ascii=False) + ',\n'
           '    restoreDefaultsConfirm: ' + json.dumps(RESTORE_CONFIRM[loc], ensure_ascii=False) + ',\n')
    s = re.sub(r'(    langRu: .*\n)', lambda m: m.group(1) + ins, s, count=1)

    # 4) mask 新分区 + langs 矩阵（都插在 export 对象闭括号 "}; " 之前）
    lang_block = '\n  mask: {\n    dragHint: ' + json.dumps(DRAG[loc], ensure_ascii=False) + ',\n  },'
    lang_block += '\n  langs: {\n' + ''.join(
        f'    {k}: {json.dumps(v, ensure_ascii=False)},\n' for k, v in zip(LANG_KEYS, LANGS[loc])
    ) + '  },'
    stripped = s.rstrip()
    assert stripped.endswith('};'), repr(stripped[-10:])
    # 末尾 '};' 含 export 语句分号；新分区插在对象闭括号之前
    s = stripped[:-2] + lang_block + '\n};\n'

    io.open(path, 'w', encoding='utf-8', newline='\n').write(s)
    print(loc, 'ok:', s.count('langs: {'), s.count('dragHint'), s.count('keyPlaceholder'), s.count('restoreDefaults'))
