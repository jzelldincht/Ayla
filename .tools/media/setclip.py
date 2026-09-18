# -*- coding: utf-8 -*-
"""经文件中转写入剪贴板（避免 PowerShell 引号转义问题）。"""
import sys, subprocess, io, os
text = io.open(sys.argv[1], encoding='utf-8').read()
p = os.path.abspath(sys.argv[1])
subprocess.run(['powershell', '-NoProfile', '-Command',
                f"Set-Clipboard -Value ([System.IO.File]::ReadAllText('{p}', [System.Text.Encoding]::UTF8))"], check=True)
print('clipboard set from', p, len(text), 'chars')
