# Changelog

Each release's packages hold the game only: no Nintendo data. You import your own copy of
Super Mario Maker 2 once (the NSP, the 3.0.3 update's NSP and your `prod.keys`) and after that
SMCraft never needs them again.

## Unreleased

SMCraft is back in development (it was archived in October 2026).

- **Import once, then no dump or keys.** SMCraft reads your NSPs and `prod.keys` itself (no
  hactool, nothing extracted) and writes its own asset pack; the game runs from the pack alone.
  On macOS the setup screen picks the files; on Windows, Linux and Android you put them in the
  data folder's `import/` folder and the first launch imports them; on iOS / iPadOS you pick them
  in Files. `mmk import --nsp <base> --update <update> --keys <prod.keys>` does the same from the
  command line.
- **Course World over TGRCode**: the course lists, course and maker pages with thumbnails,
  comments, search by course or maker ID, Play and Download. Endless and 100 Mario draw from it
  too. Your own course server stays a choice.
- **Android**: SMCraft runs as an APK (arm64, Android 9+, Vulkan), with on-screen controls.
- **Mods from the SMM2 Recomp**: SMM1's Muncher hit box, SMM1's overlapping pipes in the Course
  Maker, quick level return after a miss, and SMW's goal wipe in Super Mario World courses.
- **Network Play groundwork**: signing in to Nextendo Network and matchmaking SMCraft players
  (tested against a local server; live play waits for Nextendo).
- Fix: Munchers hurt on a wall sensor's touch, as in SMM2 (a one-tile gap between Munchers is as
  tight as in the game).
- Release packages for Windows, Linux (x64, arm64), macOS, Android and the web.
