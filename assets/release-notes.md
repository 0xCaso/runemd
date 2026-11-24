# RuneMD {{version}}

A native Markdown reader for macOS that shows what your agents write, live.

## Requirements

- macOS 26 or later, on a Mac with Apple silicon.

## Install

1. Download `RuneMD-{{version}}.zip` and open it: you get `RuneMD.app`.
2. Move `RuneMD.app` to Applications.
3. Open it. RuneMD is not signed with an Apple Developer ID, so the first time macOS says it cannot check
   the app. Close the message, then go to System Settings → Privacy & Security, find "RuneMD was blocked"
   and click "Open Anyway". On macOS 26, right-click → Open no longer skips this check.
4. After that it opens normally. To open `.md` files with it: in Finder, "Get Info" on a `.md` file,
   "Open with" RuneMD, "Change All…".

If it crashes, the log is in `~/Library/Logs/RuneMD/`: please attach it to an issue.

## Checksum

