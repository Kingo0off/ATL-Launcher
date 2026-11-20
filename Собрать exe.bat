@echo off
chcp 65001 >nul
cd /d "%~dp0"
python -m pip install pyinstaller
python -m PyInstaller --noconsole --onefile --name FriendLauncher --add-data "launcher/web;launcher/web" main.py
echo.
echo Готово: dist\FriendLauncher.exe
pause
