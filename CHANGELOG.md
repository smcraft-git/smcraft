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
- **The Course Maker**: SMM2's own maker screens (part palette and part window, styles, themes,
  timer, autoscroll, clear conditions, multigrab, stretching, test play, the Undodog and the
  rocket) with its music and the parts' sounds, and the **Coursebot** (120 slots, Save As,
  Load, Upload). Wings and parachutes from a part's attribute balloon (hold a part still).
  **Mario's Trail**: after a test play the maker shows where Mario went, faint pictures of him
  along the way as in SMM2; the left column's button switches it on and off. Any course can be opened in the maker: someone else's course saves and uploads
  as a new course, "based on <code> by <maker>".
- **Super Worlds**: play a Super World (its map, lives, progress), made from your library or
  downloaded from TGRCode.
- **Toad House minigames** in Super Worlds: Match & Win, Pop & Win and Catch & Win, with the
  game's own rules and prizes.
- **SMM2's own Endless Challenge**, and **Challenge Modes**: Course World's Endless button
  switches between it and the 100 Mario Challenge with ZL / ZR.
- **Play Together (local co-op)** for up to four players with SMM2's rules: bubbles, shared
  lives (a life goes when everyone is down), anyone at the goal clears, the first into a pipe
  waits for the others, carrying and throwing each other, ground-pound knocks, each player's own
  keys and clear-condition state; also for 100 Mario and Endless runs.
- **Network Play screens** (co-op and versus) over Nextendo matchmaking, tested against a local
  server: the match plays a Course World course (TGRCode) picked by whoever opened it, or one of
  the import's Story Mode courses; the other players show their names.
- **A Mods page** on the title screen with every mod option (SMM1 extras, the recomp's mods, Course
  World's source).
- **100 Mario: SMM1's Mystery Mushroom reward**: a finished run wins a costume.
- **Sound**: Lanky Mario's own sounds; keys, pink coins, Burners; knock-outs by what did them;
  the update power-ups' sounds and course music; splashes; boss jumps; 3D World's voices,
  footsteps, donut, blinking and spike blocks; helmets, a locked key door, a broken clear
  condition; the Course Maker's music and part sounds (moving, stacking, wings, the start and
  goal).
- **Course Maker**: wings and parachutes (hold a part still for its balloon, on a whole
  selection too), stacking enemies, tracks, pipes turned by their mouth, the parts the part
  window lacked (Spike, Pokey, Piranha Creeper, Mechakoopa, the Koopalings, P Block, Dash
  Block, the SMB2 Mushroom, the wearables, the ON/OFF Trampoline), TGRCode courses open in the
  maker, the Swift app's maker keys.
- **Android**: Bluetooth controllers' buttons.
- **Stacked enemies** move together as in the game.
- **Frozen Coins** (the 3.0 update's): ice round a coin, thawed by fireballs.
- **Koopalings on tracks.**
- **Blocks hold any part**: enemies, springs, P Switches, keys and the rest come out of ? blocks,
  bricks, hidden blocks, trees and checkpoint flags as themselves, with their wings or parachute;
  vines grow out of blocks. Bill Blasters fire any enemy or object.
- **Bill Blasters fire coins and launch what they fire as SMM2 does**: a coin blaster sends out
  ten coins at a time that arc out, bounce twice and roll; enemies and items hop out of the
  barrel (gliding for winged ones, Boos and Bloopers slowing to a stop, parachutes opening after
  the shot), three times as fast from the red blaster; each blaster keeps as many alive as the
  game allows. Fix: only coin blasters fire at random intervals.
- **Parts come out of blocks, pipes and Bill Blasters as in SMM2** (each part's own way, from
  the 3.0.3 game):
  - Out of a block, an enemy hops up from inside the block and lands on top before it walks;
    winged ones rise slowly, a winged red Koopa climbs well clear of the block, Boos and
    Bloopers float out, a Lava Bubble leaps high, a Cheep Cheep faces you, P Switches and
    springs pop higher; a parachute is shot up first and opens on the way down.
  - Out of a pipe, a parachute is shot out and opens once clear; winged red Koopas fly out,
    winged Bob-ombs and Buzzies hover at the mouth; big enemies squeeze out, get stuck for a
    second and jump free; Lava Bubbles are launched; items face right. **Coins in pipes** now
    come out as bursts of ten bouncing coins.
  - From a Bill Blaster, Spinies and Buzzies stop on landing, Lava Bubbles fly on through
    everything, P Switches and springs fly like items. The red blaster fires its next homing
    Bullet Bill about 5 seconds after the last one is gone. Fix: Bullet Bills leave the barrel
    at the right height (they came out 8 px low).
  - Parachutes swing the right way after a shot to the left, and swing on at a wall.
- **Enemies holding a Key** let it go when defeated; **Lakitus throw what they were given**,
  coins as a ten-coin burst.
- **The 3.0 update's clear conditions**: defeat Spikes, Pokeys, Mechakoopas or a Koopaling;
  reach the goal as Link, SMB2, Frog, Balloon, Flying Squirrel or Boomerang Mario, or wearing a
  Cannon Box, Propeller Box, Goomba Mask, Red POW Box or Bullet Bill Mask.
- **Tracks drawn with the game's own track tiles** (straights, bends, joints, open and closed
  ends) instead of a plain line.
- **Endless**: the map's "New High Score!" panel.
- **Physics**: 352 player traces recorded in the game match frame by frame (walls, water,
  slopes, items, the raccoon tail and SMW cape, enemies).
- **Pictures closer to the game**: the start sign, the 2D styles' drop shadow, animated
  ? blocks, bricks and coins, SMB1's black daytime ghost house, night darkness, every style's
  own backdrops, the airship's sway, NSMBU's and 3D World's colours, lights, skies and fogs as
  the game's shaders make them, Mini Mario.
- Release packages for Windows, Linux (x64, arm64), macOS, Android and the web.
