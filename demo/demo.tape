# Records the README demo GIF. Run from the repo root: `make demo` (builds first)
# Uses a throwaway HOME so your real ~/.config/tuido/todos.json is never touched.

Output demo/demo.gif

Set Shell "bash"
Set FontSize 18
Set Width 1100
Set Height 600
Set Padding 20
Set Framerate 24
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms

Hide
Type "export HOME=$(mktemp -d) && clear && ./target/release/tuido" Enter
Sleep 1s

# Seed a few tasks off-camera
Type@1ms "a" Type@1ms "Review open pull requests 4pm" Enter
Type@1ms "a" Type@1ms "Plan next sprint tomorrow at 10am" Enter
Type@1ms "a" Type@1ms "Buy coffee beans" Enter
Type@1ms "a" Type@1ms "Renew passport" Enter
Type@1ms "a" Type@1ms "Reply to Sam's email" Enter
Type "k"
Sleep 500ms
Show

Sleep 1.5s

# Add a task with a natural-language due date
Type "a"
Sleep 500ms
Type "Write release notes tomorrow at 5pm"
Sleep 800ms
Enter
Sleep 1.5s

# Complete a couple of tasks
Type "k"
Sleep 400ms
Space
Sleep 800ms
Type "kk"
Sleep 400ms
Space
Sleep 1s

# Suspend one for later
Type "k"
Sleep 400ms
Type "s"
Sleep 1.2s

# Reorder: move the top task down
Type "kk"
Sleep 400ms
Type "J"
Sleep 600ms
Type "J"
Sleep 1s

# Hide and show completed tasks
Type "h"
Sleep 1.5s
Type "h"
Sleep 1s

# Help screen
Type "?"
Sleep 3s
Escape
Sleep 1s

Type "q"
Sleep 500ms
