# The dashboard tour in the README: docs/assets/dashboard.gif
Output out/dashboard.gif

Require diskgc
Require diskgc-ui

Set Shell "bash"
Set FontFamily "JetBrains Mono"
Set FontSize 15
Set Width 1440
Set Height 880
Set Padding 18
Set Margin 24
Set MarginFill "#11111b"
Set BorderRadius 12
Set WindowBar Colorful
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms
Set Framerate 24

Hide
Type "bash /usr/local/share/diskgc-demo/setup.sh && (diskgc daemon >/tmp/daemon.log 2>&1 &) && sleep 12 && bash /usr/local/share/diskgc-demo/setup.sh && clear" Enter
Sleep 30s
Show

Type "diskgc ui" Sleep 400ms Enter
Sleep 2s
# Overview: free space, level, forecast; r fills in where the space goes.
Type "r" Sleep 4.5s

# Report: the biggest offenders per category.
Type "3" Sleep 4.5s

# Plan: every action with its reason. Keep one, execute the rest.
Type "2" Sleep 1s
Type "p" Sleep 4.5s
Down Sleep 500ms Down Sleep 500ms
Space Sleep 1.5s
Type "x" Sleep 5s

# History: runs, growth per day, repeat offenders. Log: what the daemon did.
Type "4" Sleep 4.5s
Type "6" Sleep 3.5s
Type "1" Sleep 1s
Type "r" Sleep 4s
Type "q" Sleep 500ms
