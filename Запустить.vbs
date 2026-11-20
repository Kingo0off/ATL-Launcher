' Запуск без консоли. Подходит, если двойной клик по FriendLauncher.pyw не работает.
Set sh = CreateObject("WScript.Shell")
sh.CurrentDirectory = CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName)
On Error Resume Next
sh.Run "pyw -3 main.py", 0, False
If Err.Number <> 0 Then
  Err.Clear
  sh.Run "pythonw main.py", 0, False
End If
