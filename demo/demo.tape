# hauntty demo — record with:  vhs demo/demo.tape   (run from the repo root)
# Produces demo/hauntty.gif. Uses a throwaway copy of demo/ghostty-config so the
# committed sample is never modified.

Output demo/hauntty.gif

Set Shell "bash"
Set FontSize 17
Set Width 1180
Set Height 720
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 85ms

# Launch hauntty against a throwaway config (startup hidden).
Hide
Type "cp demo/ghostty-config /tmp/hauntty-demo.conf && ./target/release/hauntty --config /tmp/hauntty-demo.conf"
Enter
Sleep 1.5s
Show

# Browse: the live preview follows the highlighted theme.
Sleep 1.8s

# Filter to the Tokyo Night family.
Type "/"
Sleep 500ms
Type "tokyo"
Sleep 1.6s
Escape
Sleep 500ms

# Pick a theme; watch the preview update.
Down
Sleep 900ms
Down
Sleep 1.2s

# Apply it — hauntty offers to save the current inline colors first.
Enter
Sleep 2.2s
Type "y"
Sleep 2.4s

# Settings tab: friendly widgets, no raw config editing.
Tab
Sleep 1.2s
Down
Sleep 400ms
Right
Right
Sleep 1s
Down
Sleep 400ms
Left
Left
Sleep 1s

# Save settings.
Type "s"
Sleep 1.8s

# Back to Themes, then fetch more themes from the upstream catalog.
# The GitHub request runs on a background thread — the UI stays responsive
# and a spinner animates while the list loads.
Tab
Sleep 800ms
Type "f"
Sleep 3.5s
Type "gruvbox"
Sleep 1.6s
Enter
Sleep 2.6s
Escape
Sleep 800ms

# Starship tab: browse and apply a prompt preset with an automatic backup.
Tab
Tab
Sleep 1.2s
Down
Sleep 600ms
Down
Sleep 600ms
Down
Sleep 800ms
Enter
Sleep 2.2s

# Filter presets.
Type "/"
Sleep 400ms
Type "tokyo"
Sleep 1.4s
Escape
Sleep 500ms
Down
Sleep 700ms
Enter
Sleep 2s
Escape
Sleep 800ms

# Help overlay.
Type "?"
Sleep 2.2s
Escape
Sleep 500ms

# Quit.
Type "q"
Sleep 800ms
