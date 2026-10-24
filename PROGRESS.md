# Progress

## Goal (owner, 2026-10-21) — SMCraft revived
SMCraft is back in development (it was archived on 2026-10-19 in favour of the recomp; both now go on).
The goal: **Super Mario Maker 2 rebuilt from scratch**, every style as close to pixel-, physics- and
sound-perfect as possible, that **never needs the ROM or key files to run**.
- **Assets:** a one-time import reads the player's own dump (NSP + `prod.keys`, plus the 3.0.3 update)
  and writes a local asset pack (art, sound, layouts, fonts, text, course key table). After that the
  game runs from the pack alone; the dump and keys can be deleted. No Nintendo files in this repo.
- **Code:** clean-room. Behaviour is worked out from the binary (Ghidra / REA), written up as prose
  specs, implemented as original code and checked frame by frame against the real game in Ryujinx.
- **Modes:** Network Play through Nextendo; Course World search and play through TGRCode; the
  Course Maker for any course (edit someone else's course, SMM1 style); 100 Mario Challenge;
  Endless Challenge; local co-op (courses and the challenge modes); the smm2recomp mod options;
  Super Worlds. **Not** Story Mode (its core stays in the tree, its screens aren't built).
- **Platforms:** the same as smm2recomp: macOS, iOS / iPadOS, Windows, Linux, Android, plus the web.

### Resumed 2026-10-24 (turns of ~50 minutes with SMM2) — where this stands / next
Everything is on `main` and pushed (`physics3` and `pixels5` merged 2026-10-24: 286 / 286 traces
exact over 13 re-recorded courses, SIM_VERSION 2; night darkness / SceneDB backdrops / airship
sway, importer and pack 12, the dev cache already at v12).
- **Since the resume:** `maker-audio` / `maker-hooks` merged (pack 11); 100 Mario's costume reward;
  Lanky Mario's sounds; co-op per-player keys, carrying, the ground-pound knock, per-player action
  conditions and the 180-frame warp wait; enemies' defeat sounds by cause and many update
  power-up / 3D World sounds; Network Play on a TGRCode course the session's opener picks; the
  Endless map's "New High Score!" panel.
- **Waiting on the owner:** the SMCraft app on Nextendo's portal (public client / PKCE, scopes
  `identity game.matchmaking`, redirect `http://127.0.0.1:47614/callback`) and its client ID; the
  game-server access key — (a) read from the player's own dump at import (recommended) or (b) one
  Nextendo issues. Nextendo has agreed to SMCraft going online. Releases need the owner's OK.
- **Next, in order:**
  1. Physics (352 / 352 exact over 26 courses, enemy stomp / die included; the trace Ryujinx
     boots again from /private/tmp copies — a macOS privacy prompt hung it on ~/Documents and the
     SSD): the 3D World courses (a MoltenVK build without 3D texture compression is ready,
     `ryubuild/no3dbc`), Koopa / shell traces, WU form. Pixels (3D World mean 46.0 → 33.0;
     NSMBU day 38.5 → 39.9): NSMBU's TEV colour stages, the light-map lookup domain, specular /
     env-map reflections, rim light, baked AO (needs an importer bump), the other sky variants,
     3D World snow, the colour table.
  2. Maker: ✅ wings / parachutes (the attribute balloon, merged 2026-10-24), ✅ over a multigrab
     selection (hold a selected part: wings / parachute go to every part that takes them, one
     undo step); a part put into a block keeps its wings / parachute / form / size as the
     contents' flags ✅; left: play side — a winged / parachuted item coming out of a block
     (`blocks::Contents` keeps only the 0x4 form; its behaviour wants the binary), NSMBU / 3D
     World wing art, the contents balloon (`Edit_BalloonPartsIn_00`, state `cDispInParts`: not
     built at all); tracks and links.
  3. Co-op: a players screen in SMM2's layouts; the P-door hold for followers.
  4. Super Worlds' 3D map and Toad Houses; airship sway; the colour table; NSMBU / 3D World pixels.
  5. iPad / Android device runs; the macOS package; live Network Play once the ID and key arrive.

### Plan
✅ done · 🔄 in progress · ⬜ not started · ⏳ waiting on owner

**G0 — Revive** 🔄
- ✅ Clone of github.com/smcraft-git/smcraft (snapshot `8bdb0ff` = tag `smcraft-final` without
  `recomp/`); builds, 763 tests pass
- 🔄 Goal and plan written down (this section, DECISIONS.md 2026-10-21); README no longer "archived"
- ✅ Brought over the course-server work done on smm2recomp since the split (`smcraft-final..main`):
  the TGRCode upstream fetcher (`server/src/upstream.rs`, `mmk server --upstream tgrcode`),
  thumbnails and whole-course maps on the server, Played / Liked lists, `based_on` uploads, the
  listener that recovers from a failed accept, the `mmk-embed` crate, Endless's run slot per
  ruleset and no-repeat pool, the relay's 1450-byte messages; licence GPL-3.0-or-later
- ⬜ CI on the public repo (Linux on push; macOS / Windows / wasm / Android by hand)

**G1 — Import once, then ROM-free** 🔄
- ✅ The importer reads the dump itself (`mmk import --nsp <base> --update <update> --keys
  <prod.keys>`, `core/crates/import/src/nsp/`): NSP (PFS0) → NCA (header AES-XTS, sections AES-CTR,
  title key from the ticket) → RomFS / ExeFS, the update's BKTR patch over the base, `prod.keys`
  read once and never stored; no hactool. Every RomFS / ExeFS file matches the reference
  extractions, and the import matches the folder import's caches byte for byte (DECISIONS.md
  2026-10-21)
- 🔄 One asset pack for everything: one run writes the game cache and the SMM2 UI (gameplay,
  sound, modes data, effects, UI) into one asset root ✅; the runtime checks each pack's importer
  version (`mmk_host::paths::GAME_PACK_VERSION` / `UI_PACK_VERSION`, kept equal to the importer's by
  a test): an older import counts as missing and the setup asks for the dump again ✅; incremental
  re-import ⬜
- 🔄 A guard test (`core/crates/host/tests/rom_free.rs`): no code outside the importer and the setup
  screens opens NSP / keys ✅, a built-in course plays from the pack alone ✅; menus and sound run
  with no dump anywhere (an app-level run) ✅ (the second launch above)
- 🔄 Setup screen on every platform (pick the NSP, the update NSP, `prod.keys`; progress; done):
  macOS app ✅ (NSP files or extracted folders, `mmk_import_nsp`); web (`mmk-play` in a browser: a
  folder with the NSPs and `prod.keys`) ✅ builds, not yet tried in a browser; iOS / iPadOS 🔄
  (the NSPs and prod.keys picked in Files or dropped into the app's `import` folder, `mmk_import_nsp`;
  builds for the Simulator, not yet run end to end on a device; a cache made on a Mac still works); native `mmk-play` (Windows / Linux / macOS) and Android
  ✅ through a drop folder: the NSPs and `prod.keys` copied into `<data folder>/import/` are
  imported at the first launch on a thread, with the progress over the demo strip, then the front
  end starts (`shell/src/setup.rs`, `launch.rs`; checked on macOS from an empty data folder:
  import → title, then the second launch from the pack alone) · a file picker ⬜

**G2 — Course World on TGRCode** 🔄
- ✅ TGRCode client (`mmk_online::tgrcode`, endpoints checked live 2026-10-21): course / maker
  by ID (the game's checksum first), Popular / New / Endless lists, a maker's posted / liked /
  played / first-clear / record lists, comments, Super Worlds, downloads decrypted with the
  pack's course key table, thumbnails; polite (one request at a time, 2 s apart, 429 waited
  out), cached on disk, offline answers from the cache; read-only calls in the server's JSON
  shapes (`tgrcode::call`)
- ✅ Course World's source a setting (`mmk_online::world::Backend`: TGRCode default / own server /
  library; `--course-world`, `MMK_COURSE_WORLD`, the app's Settings > Game > Course World,
  `mmk_front_set_course_world`)
- ✅ Screens over TGRCode: the tabs are TGRCode's lists (Hot = a fresh Endless draw, Popular =
  course points at normal, New), thumbnails in the rows (JPEG → texture), the open row's
  Download (TypeBtn 3) into the library, the course page with its picture and whole-course map,
  tags and the record holder / first clearer, comments (text ones), the maker's profile with
  their uploads; the hub's ID button (course ID → the course, maker ID → the profile; typed into
  the comment window)
- ✅ Play without a pause: an opened row's course is fetched in the background (`Host::prefetch_course`;
  TGRCode's client caches it), so Play loads from the cache
- ⬜ Popular across difficulties / TGRCode's sort options; more than 20 New courses (paging)
- ⬜ Leaderboards / Ninji where TGRCode has data; Super Worlds screens (G5; the client has
  `super_world`)

**G3 — Challenges** 🔄
- ✅ 100 Mario Challenge (SMM2 and SMM1 presentation, rules from SMM1)
- 🔄 Endless Challenge: rules ✅; SMM2 screens on SMM2's own rules ⬜; courses from TGRCode ✅
  (`mmk_embed::pool`: the ported upstream fetcher fills a local store from TGRCode's Endless pool
  when the runs open, 10 per difficulty per launch; the runs draw from it and the library, and
  play what it holds when offline)
- 🔄 Local co-op for courses, 100 Mario and Endless (SMM2 local rules: bubbles, shared lives;
  research in `research/specs/local-coop.md`, private)
  - ✅ Sim (`world/coop.rs`, `Camera::update_players`): every player runs the same player rules
    (the lead runs the world tick, the others the player's part of it); a death while another
    plays comes back in a bubble (floats after the nearest player, Jump dashes, pops on touch,
    127 frames of invincibility); a life is lost only when nobody is in play (one shared life);
    one camera over the players in play (spread blend, peeks, join weight), its edges push
    players along, squeezed / below the view = death; head springboard and stomp bounce, side
    push; the goal waits 240 (+30) frames, one player at the goal clears; a warp takes everyone
    (out 15 frames apart); time up kills everyone. One player steps exactly as before (traces,
    a test). 9 tests
  - ✅ Front end: Play Together on the Coursebot's own button (`L_CoopModeBtn_00`, My Courses)
    and on the Course World / library rows (the report button's slot with the co-op label and
    icon: the co-op slot holds Make there); the controller step takes one player per connected
    controller, 2–4 (a message with only one); `Host::set_players` / `Host::controllers`,
    `CourseStatus::players`, "dying" = everyone down (`World::all_down`, `restart_in`), the
    HUD's player icons (`TypeMember`). Checked: a capture of the Coursebot window with the
    button; a headless three-player run (`mmk-play --players 3 --input-script`): player 1 dies,
    comes back in a bubble from the right, floats to Luigi, pops and jumps out
  - ✅ Co-op 100 Mario / Endless: with 2+ controllers New Game / Continue asks "How many
    players?" (← / →, confirm); the run's courses start with them; one shared life counter
    (a death is everyone down)
  - ✅ mmk-play: the player count stays across restarts and courses (unloading goes back to
    one); pads by player, the keyboard is player 1 on its own with fewer pads than players
  - ✅ The Swift app / FFI front end: the controllers and their buttons from the app (not yet
    played on a device)
  - ✅ Carrying a player (not SMB1): Run held + Spin beside a grounded player in front, ridden on
    the head; free after the 300-frame hold with Jump / Spin, or with 12 presses after 60 frames;
    letting go of Run throws (vy 4.5, ≥ 3.0 across, 10 frames apart), Down puts down
  - ✅ Keys per player (a key door needs the entering player's own); a stomp / springboard
    between players moves the victim's keys to the attacker
  - ✅ Bubble / pop sounds (`<style>_Balloon` `act_balloon_start` / `act_baloon_break`)
  - ✅ A ground pound knocks another player flying (vy 4.0, 1.0 / 1.4 away; no harm, no control
    until landing), and takes their keys
  - ✅ Action conditions per player (no damage / landing / claw / water): one player breaking
    the rule leaves the others able to clear; each pass checks only its own events
  - ✅ Warps (§7): the first into a pipe / door waits inside up to 180 frames while the others
    play on (and may follow); with nobody else free it goes at once; then everyone is taken
    along, also through a warp within the area; a player sinking into a pipe is drawn behind it
  - ⬜ Not yet: a players screen in SMM2's layouts (the question is a toast); the P-door hold
    for followers (§7.5)

**G4 — Course Maker for any course** 🔄
- ✅ Editor core (`mmk-editor`): parts, areas, links, tracks, selection, settings, the game's limits
- ✅ Research: the maker scene from the v3.0.3 data + binary (private `research/specs/maker.md`
  Part C: `Edit_MainPage_00` and its parts, the part window `Edit_RingMenu_00`, palette slots,
  right/left columns, main menu, Save/Load, the editor models' textures, sounds; open items)
- 🔄 SMM2's maker screens in `mmk-front` (`maker`), over `mmk-editor`, the course drawn by the host
  as a still view (`Host::edit_show` / `edit_camera`) — shell and Swift-app hosts both:
  - ✅ title Make → clap + DecideEditBtn → the maker (the draft reopens, else a new SMB1 course)
  - ✅ `Edit_MainPage_00`: palette bar (12 slots with the parts' pictures drawn from the engine's
    art, `mmk_host::icon`; selected slot, category colour), left column (style logo, theme and
    autoscroll pictures, timer, clear condition icon + amount), right column (main menu,
    Save/Load, eraser toggle, Reset Rocket, Undodog in once there is something to undo),
    clapperboard, bottom scroll bar (S / G, area button, handle); each part's In
  - ✅ over the course: grid, the frame round the part / tile under the cursor, the glove and the
    part it carries (the import's `EditModel` textures: import v5 adds the editor models'
    textures)
  - ✅ placing / painting (ground, blocks, coins while held) / erasing (eraser, back held, right
    mouse) / picking a part up and dropping it elsewhere (a tap on a part: its other form);
    parts put on (or dropped on) a container go inside, a Super Mushroom makes a part big; one
    undo step per press; undo / redo; mouse, touch taps, keyboard and pad (README keys)
  - ✅ part window `Edit_RingMenu_00`: four categories × pages of ≤ 13 round the ring (the ring's
    own Type / Select / Category animations), names from `PartsIconName`; the choice goes to the
    front of the palette [page contents ours: not in the game's data]
  - ✅ left column windows: Game Style, Course Theme, Timer (selector, OK), Autoscroll (none /
    slow / normal / fast), Clear Condition (`Edit_MissionWindow_00`: tabs, condition and amount
    selectors with the HUD's icons and sentences, Remove)
  - ✅ test play: from the maker's Mario (the start, then where the last test play ended), held
    for the clapperboard's HoldWait from the start, T from the cursor; SMM2 HUD; back on death,
    clear or the key
  - ✅ main menu `Cmn_MainMenu_00` (Back to Title, Course Maker, Coursebot, Story Mode, Course
    World); Save/Load `Edit_SaveLoadMenu_00` (Save, Save As, Upload as the signed-in account,
    Load → the Coursebot) → the Coursebot (`Host::save_course`: `<data>/Coursebot/course_NNN.bin`,
    in memory for captures; the app: `Coursebot/` next to its downloads) → listed in the
    Coursebot / Course World lists and playable there
  - ✅ captures `mmk-play --front-screen maker[-parts|-menu|-save|-style|-theme|-timer|
    -autoscroll|-mission|-play]` with `--front-script` presses, maker keys, cursor tiles, slots
  - ✅ dragging the start (its height) and the goal (its height, the course's length);
    stretching parts by their right column / top row; multigrab (select, carry, copy, erase);
    the sub-area turns vertical from the scroll bar
  - ✅ wings and parachutes (2026-10-24; `research/specs/maker.md` C10, from the v3.0.3 binary:
    there is no Wing / Parachute part): a placed part held still for 16 ticks opens its
    attribute balloon `Edit_BalloonPartsChange_00` over it (turned away from the screen's
    edges) — its forms (the current one checked), big / small, wings on / off, parachute on /
    off, each button the part's picture as the choice would make it (`Host::part_icon` with
    flags), greyed when a part limit forbids it; d-pad / confirm / back or taps. Editor core
    `mmk_editor::attr` (`attr_options`, `apply_attr`, `set_wings`, `set_parachute`): wings and
    parachutes only on parts that can carry them, no parachute on a part riding a Clown Car,
    track or Lakitu's Cloud or stacked (the game's 0x31600 mask), limits re-checked (a winged
    block counts with the enemies). Sounds `voice_getWing` / `lose_wing` / `p_EquipPara` /
    `p_RemovePara` / `scaleChange` / `p_AttributeChange` + the balloon's UI keys. Wings drawn
    on winged actors in the maker's view, its icons and play (`Enemy_wingcommon`, flapping
    while they move); parachutes were drawn already. One undo step per choice.
    ✅ multigrab selections (2026-10-24), contents keep their flags · ⬜ the "what is
    inside" balloon with its dummy wings / parachute, wings on the NSMBU / 3D World models
    (their own textures; SMB1 art stands in), parts that drop wings / parachute with their
    other form (vtables not resolved), winged Bill Blasters in play
  - ⬜ custom autoscroll and its nodes, water level, the sub-area set-up window, the vertical
    scroll bar, the part variants balloon (Fast Conveyor …), tracks / doors' partners placed
    by hand, pipes turned by their mouth, the trash can, Mario's Trail, co-op making
  - ✅ maker music and part sounds (2026-10-24; `research/specs/maker.md` C9): the style's
    `BGM_<Theme>_Edit` for the area's theme while editing (a theme / style switch changes it,
    test play stops it before the course's own starts, back it plays again, leaving the maker
    stops it); parts' `<S>_Edit<Part>` sounds: put down (+ the part's voice [inf]), picked up,
    dropped, erased, refused (`partsMax` / `invalid`), put inside (`Block_in`, `Dokan_in`,
    `Killer_in`, `Jugem_in`, `Clown_in`, `Cloud_in`, `Key_In`), made big (`scaleChange`), form
    change (`p_AttributeChange`), multigrab frame / grab / drop / erase. Host hooks
    `edit_music` / `edit_sound`, routed in the shell and the FFI (so the Swift app has them) to a
    maker `Director` of their own. ⬜ the selection frame's `cursor` ticks, `move`, the voices'
    `MusicalScale` pitch by height, wings / parachutes (`p_EquipPara` …), the start / goal and
    stretch drags' sounds, a listen on hardware against SMM2
- 🔄 Edit any course: the open row of the Course World / Coursebot lists has "Make" (the co-op
  button's slot, the Coursebot's word, the glove icon) → the maker; someone else's course (a
  code whose maker isn't the signed-in player) saves as a new Coursebot course with "based on
  <code> by <maker>" in its description, and so uploads ✅ · TGRCode courses ✅ (the hosts'
  `course_data` downloads through Course World's backend, as play does; checked: a live TGRCode
  course downloaded, decrypted and parsed through it)
- ✅ Coursebot screens `Lcl_CourseList_00` / `Lcl_CourseListContents_00` /
  `Lcl_CourseInfoWindow_00`: 120 My Courses slots with names and pictures (the start view drawn
  from the engine's sprites), Downloaded Courses (the library's others), the course window (Make,
  Play, Upload, Name, Description typed, Delete; Make New Course), play from it; the maker's
  Save As ("Select Save Location", overwrite question) and Load ("Select a Course to Load") ·
  ⬜ moving slots, the Super World filter, the download flow from Course World, CJK glyphs in
  names
- 🔄 The Swift app: its host has the maker and Coursebot hooks ✅; a finger / mouse held down
  reaches the maker (paint, carry, erase) ✅; the maker keys from its keyboard ⬜; 3D styles'
  still view uses the play camera ⬜

**G5 — Super Worlds** 🔄
- ✅ Research (`research/specs/super-worlds.md`, public format `docs/formats/smm2-super-world.md`):
  the world-map blob (0x500 bytes, encrypted like a course with its own key table; TGRCode's
  `unk1` and the game's `MyWorld/new_layout_data.ldt`), the 7 × 4 grid with its road pieces and
  connections, points, pipes, Toad Houses, course order, themes, the progress record and its
  checks, `myworld.dat`'s slots, the screens' layouts. Checked on the template and 5 TGRCode
  worlds (39 worlds): byte-exact round trips, every point reachable, every link two-way
- ✅ Format and rules: `mmk_formats::smm2::world` (crypto, key-table search, map parse / write),
  `mmk_modes::superworld` (test world from the library, TGRCode worlds, walking and blocking,
  clears, lives, game over, Toad Houses, world clear and end, saved progress)
- ✅ Import stage `myworld` (`keys/world_table.bin`, found with the dump's own `.ldt`;
  `mmk import-myworld` adds it to an existing pack)
- ✅ Play: Course World's Super Worlds button → planets (`Nwk_MyWTitle_00` over the hub's
  InMyWTitle backdrop: the library's test world + TGRCode's picked-up worlds) → title
  (`MyWEdit_WorldPlayTitle_00`, New Game / Continue) → the map (flat 2D drawing under
  `MyWEdit_PlayScene_00`'s HUD, course-name balloon, Play prompt) → courses with lives (card
  `AnimMyWorld` with "W1") → `MyW_Ending_00`. Progress saved per world (`superworld.<id>`).
  TGRCode: `super_worlds` / `super_world` calls (`mmk_online::world::Backend::super_world`
  decrypts the map); captures `superworld`, `-title`, `-map`, `-walk`, `-play`, `-ending`
- ⬜ The map in 3D with the game's models (`MyWorld_Model.pack`: fields, roads, points, Mario's
  walk / swim / climb animations from `WM_*Anime`), the map music and sounds
- ⬜ Toad House minigames (`MyWEdit_Minigame_00…02`; a fixed 3-life gift for now), enemy-icon
  courses that start on contact, point icons by model, map ghosts, co-op ("Play Together")
- ⬜ Super World maker (later)

**G6 — Network Play through Nextendo** 🔄 (works end to end on a local server; live waits for
Nextendo; `docs/nextendo-network-play.md`, spec `research/specs/network-play.md`)
- ✅ NEX client in Rust (`core/crates/nex`, `mmk-nex`): WebSocket + TLS with the server's key
  pinned, PRUDP Lite (handshake, reliable order, fragments, acks, pings; V1 codec), RMC and NEX's
  types, the ticket, ValidateAndRequestTicketWithParam, the secure connection, Register.
  Signatures and layouts byte-checked against `nextendo-nex`'s own code
- ✅ Local test server: `scripts/nex-local.sh` builds Nextendo's public server core (fetched,
  never committed) with our `tools/nex-local/main.go`; 6 integration tests (skip without it),
  one of them a cancelled search leaving its session
- ✅ Matchmaking for SMCraft-only sessions (own game mode, attributes, course hash; never meets a
  console's), participants, notifications; NAT check; probe requests and our UDP hole punch →
  peer addresses for `mmk-netplay` (`play::find_match`)
- ✅ Sign-in plumbing (`mmk-online::nextendo`): OAuth + PKCE, 127.0.0.1 callback, refresh
  rotation, `/api/nex-token`; tokens only in the host's store (`mmk-play --nextendo-sign-in`)
- ✅ `mmk-play --nextendo-local <addr>`: local server → match → co-op over rollback (with the
  headless `peer` example: 1500 frames, 24 state checks, no desync)
- ✅ Research from the binary (v3.0.3): NetworkMatchingSeq's states, the internet matching
  sub-sequence (44 states: search, fixing, lobby beat, difficulty vote, course download, battle
  start, disconnects), the in-course multiplayer sequences and the end-of-course states, the
  layouts and texts (`research/specs/network-play.md`, private)
- ✅ Network Play in the front end (`mmk-front` `netplay`, the host's `Network` trait;
  `mmk-shell` `network`: `mmk-nex` on a thread): the hub's Network Play button → the mode select
  (`Nwk_MatchingModeSelect_00` over `Nwk_MatchingBG_00`), Co-op and Versus; the wait screen
  ("Finding players…", n/m found, Quit cancels and leaves the session); the lobby (`Nwk_Matching_00`,
  "Meet your team…" / "Meet the competition…", a card per player, characters by position); "Getting
  course data…"; the course with the HUD and the 3-2-1-GO countdown; the multiplayer menu (Give Up,
  the course goes on); the clear (co-op's network banner with the course and time, versus's YOU
  WIN! / YOU LOSE); the result (`Play_BattleResult_00`: Exit / Continue playing = a new search); a
  peer leaving (goodbye datagram, or 5 s of silence) ends the match with a message. The sign-in
  state on the mode select (Standings's place): sign in / out (a test account on the local
  server). The course: every player draws the same one from the Story Mode courses of their
  import by the gathering ID, the pool's fingerprint in the session attributes. Checked: headless
  captures of every screen (`--front-screen netplay…`), a flow test on a mock network, and real
  local matches between two `mmk-play --nextendo-local` front ends (co-op and versus: 600 frames,
  9 state checks, the same player positions on both, no desync; a peer quitting → the other back
  to the mode select; a cancelled search)
- ✅ Course World courses: with TGRCode's Endless pool on disk every player searches with a pick
  of their own (a course code after the course hash in the session's application data, pool
  `WORLD_POOL`); the opener's pick is everyone's course, downloaded by code. Story Mode pool
  without. Checked: a front flow test, the local-server end-to-end test (both get the opener's)
- ⬜ Difficulty vote and TGRCode courses by difficulty (the owner sends the code); Miis / names
  of the other players (peers' names aren't exchanged: "Player n"); versus rating; watching mode
  after one's run ends; preset messages; Play with Friends (rooms); 3–4 players across NATs (the
  front end asks for 2); a match going on without a player who left; a relay for symmetric NATs
- ⏳ Live: Nextendo's OK and an SMCraft app registration (or the SMM2Recomp one with loopback
  redirects), the game server's address / key pin, the access key (from the import, or one of
  SMCraft's own from Nextendo); then the shell's `NetworkHost` gets a live server kind (the OAuth
  sign-in of `mmk-online::nextendo` behind the mode select's account button). Until then Network
  Play exists only with `--nextendo-local`; without it the screens say it isn't set up
- ⬜ Live test with the owner (needs their go-ahead and a second player)

**G7 — The smm2recomp mod options** 🔄
- ✅ SMM1 extras (shell kicks, layering, Lanky Mario, costumes) as engine options
- ✅ Muncher hit box (SMM1): `muncher_smm1_hitbox` (sim option): a 12 × 12 box for the player.
  On the way, SMM2's own rule fixed: a wall sensor's touch hurts (0.1 px a side in a one-tile gap)
- ✅ Overlapping pipes (SMM1): `Editor::smm1_overlap_pipes`, a pipe may go over a pipe
- ✅ Quick level return: `mods.quick_retry=1` (`mmk-play --mod quick_retry=1`): the death sequence runs
  8 ticks a frame, about 0.4 s instead of 3.1 s
- ✅ SMW goal wipe (on by default, `mods.smw_goal_wipe=0` turns it off): the circle and SMW's own
  goal-wipe sound (imported with the UI) before the clear panel in SMW courses
- ✅ Challenge Modes (`front/src/challenge_modes.rs`): Course World's Endless Challenge button reads
  "Challenge Modes" and opens the mode used last; ZL / ZR (keys 1 / 3, or a tap on the title bar,
  "ZL < mode > ZR" in the font's own glyphs) switch between the 100 Mario Challenge (on the screens
  the Mods page chose) and SMM2's own Endless Challenge, new in the front end
  (`front/src/endless_run.rs`: `mmk_modes::endless::Session` offline over the library and the
  TGRCode pool, 5 lives, 1-Ups paid at the goal, skips, Continue, saved as `endless`; the map's 32
  dots go round). L / R (Q / E) change the difficulty tabs on both menus. Off on the Mods page: the
  button keeps its name and opens SMM2's Endless alone. The map's "New High Score!" panel after a
  clear past the old best (`N_BreakRecord_00`, InRanking / LoopRanking; capture `endless-record`).
  The goal time bonus is counted once, through the course's own time tally (50 a second, as the
  spec's bonus) [?: whether SMM2 adds it again on top: 0x71012c7bf0]. Endless still open: the
  server's runs and rankings
- ✅ One Mods page in the front end (`front/src/mods.rs`, `mods_page.rs`; every platform:
  `mmk-play` desktop / web, Android, the Swift app's front end): SMM2's settings window
  (`Cmn_OptionWindow_00`) over the title, opened from SMM2's round menu button in the title's
  top-left corner (up from Make / Play, or a click / tap). Two pages (L / R, or its page row):
  shell jumps, draw order, SMB1 Super Mushrooms, the Mystery costume, the Muncher hit box,
  overlapping pipes; quick level return, the SMW goal wipe, the 100 Mario screens, Course World's
  Endless button, Course World's courses. Each with its description; options that change play
  are marked * and say they stay off online and send no record / replay (the hosts already
  enforce it). Kept through the host (`mods.<name>`, `course_world`), engine options handed to
  the sim at start and when changed (`Host::set_engine_option`), Course World's source switched
  live (`Host::set_course_world`). Swift app: the same options in its SwiftUI Settings, one store
  (`FrontStore` maps the page's keys onto its UserDefaults). Captures: `mods`, `mods-2`,
  `challenge-modes[-hundred|-endless]`, `endless`, `endless-map`. The front end's Course Maker
  takes overlapping pipes from it when it opens (the shell's old `--edit` maker doesn't)
- ⬜ Mid-air shell jumps checked against the recomp's test course; SMM1 course card choice

**G8 — Fidelity: pixel, physics, sound** 🔄
- 🔄 Physics: re-recording every trace (2026-10-24; the old 428 / 454 was not a true count — the
  old items / form traces were all small Mario). Re-recorded so far, current engine: **352 / 352
  exact** over 26 courses: M1/M3/MW/WU flat 34 each, M1/M3/MW/WU items 34 each, M1/M3/MW/WU
  slope 4 each, M1/M3/MW/WU wall 4 each, M1/M3/MW/WU water 2 each, M3/MW form 16 each,
  M1/M3/MW/WU enemy 2 each (a stomp on a penned Goomba with the held-jump bounce; a running jump
  from against the pen wall onto Munchers: the death, its 45-frame freeze and the hop) (was 274
  of the first 286 with the engine at 39fdb6e). Fixed: pit death at view bottom − 56 with no hop,
  slip end on flat, raccoon tail wag, SMW cape take-off / climb end / slide end, jump bonus from
  the pre-move speed, the wall probes at the moved height (rising past a wall's top corner frees
  the player a frame earlier), solid actors' sides (Munchers) flush like terrain; 3D World
  slide-end stoop 3 frames (SquatStart's length + 1, as NSMBU's 9 → 10; not traced). ⬜ still to
  record: the 6 3D World courses incl. 3W_enemy (headless Ryujinx aborts in MoltenVK: a Metal
  texture-descriptor validation failure in vkCmdBlitImage), WU form (the Propeller Mushroom is
  never collected at the title screen: floor, block, open pen and a roofed box with a doorway all
  tried). Then Koopa / shell and other object traces; whole courses start to goal
- 🔄 Pixels against the game (smm2recomp reference frames of `mmk ref-courses`, all 90, update
  900; MAE / % pixels off by > 24; `research/tools/pixel-ref/pixref.py`, 2026-10-24):

  | style | ground | underground | castle | airship | water | ghost | snow | desert | sky | forest | mean |
  |---|---|---|---|---|---|---|---|---|---|---|---|
  | SMB1 | 0.8 / 0.5% | 0.6 / 0.4% | 4.0 / 2.9% | 4.7 / 3.8% | 1.8 / 0.8% | 0.6 / 0.4% | 1.0 / 0.6% | 0.9 / 0.5% | 4.2 / 5.7% | 4.4 / 5.6% | 2.3 |
  | SMB1 night | 1.3 / 1.0% | 0.8 / 0.6% | 4.2 / 3.1% | 4.6 / 3.7% | 0.2 / 0.1% | 0.2 / 0.1% | 0.6 / 0.5% | 1.0 / 0.8% | 1.2 / 0.9% | 3.3 / 3.2% | 1.7 |
  | SMB3 | 2.7 / 1.3% | 2.6 / 2.9% | 5.0 / 4.9% | 10.6 / 17.5% | 2.5 / 1.7% | 1.4 / 1.3% | 2.0 / 1.4% | 2.2 / 1.2% | 9.7 / 27.1% | 11.2 / 14.1% | 5.0 |
  | SMB3 night | 1.8 / 1.6% | 2.1 / 4.3% | 4.5 / 5.0% | 5.0 / 5.4% | 0.6 / 0.8% | 0.7 / 0.9% | 1.3 / 1.1% | 1.8 / 1.7% | 1.7 / 1.5% | 5.5 / 5.1% | 2.5 |
  | SMW | 2.5 / 1.9% | 2.4 / 2.3% | 4.7 / 5.7% | 17.0 / 19.9% | 3.1 / 3.5% | 1.4 / 1.1% | 2.6 / 2.0% | 2.6 / 1.9% | 19.2 / 21.4% | 5.3 / 4.7% | 6.1 |
  | SMW night | 2.4 / 2.5% | 2.9 / 5.2% | 5.0 / 6.2% | 5.6 / 5.8% | 1.1 / 1.6% | 0.6 / 0.9% | 2.2 / 2.5% | 2.6 / 2.6% | 2.3 / 2.1% | 4.9 / 6.0% | 3.0 |
  | NSMBU | 50.1 / 71% | 15.9 / 46% | 26.5 / 49% | 62.6 / 77% | 47.9 / 73% | 24.8 / 60% | 38.0 / 66% | 22.2 / 51% | 60.8 / 73% | 36.2 / 63% | 38.5 |
  | NSMBU night | 41.7 / 78% | 41.6 / 76% | 46.4 / 88% | 79.9 / 87% | 5.8 / 12% | 4.5 / 11% | 40.8 / 76% | 41.6 / 74% | 40.7 / 75% | 56.0 / 91% | 39.9 |

  3D World light pass (branch light3w, the uber shader read from its own programs,
  research/specs/render-3d.md): the sky material as the game computes it and g3d's "Maya"
  texture SRT → ground 38.6, underground 34.3, castle 75.9, airship 26.5, water 32.8, ghost
  31.8, snow 70.6, desert 48.4, sky 24.6, forest 48.4 (mean 43.2, was 46.0; NSMBU unchanged).
  World-projected textures for every `tex_coord0_calc` 5 material (the field's soil bands now
  64 px tall, as the game's), phase ½ → ground 37.6, underground 34.3, castle 75.8, airship
  26.5, water 32.7, ghost 29.1, snow 70.4, desert 48.0, sky 24.6, forest 48.3 (mean 42.7).
  The albedo combiner (form stages: vertex colours added / multiplied), vertex colours tinting
  the light only with `enable_vertex_color`, the four fogs by name with their calc types (blend /
  add / multiply) and squared distance density, the lit colour clamped to 0 … 2 → ground 34.4,
  underground 19.3, castle 31.8, airship 29.5, water 35.6, ghost 22.2, snow 74.2, desert 24.9,
  sky 17.0, forest 41.2 (mean 33.0). NSMBU day 38.5 → 39.9 (ground 50.1 → 41.4, sky 60.8 → 68.9,
  airship 62.6 → 71.2: its TEV colour stages are not drawn yet; a linear fog would give 37.5),
  night 39.9 → 39.7. Next: the light-map LUT domain (Lambert vs wrapped: wrap ½ measures 30.2),
  NSMBU's TEV stages, specular + 3W_EnvMap reflections, edge (rim) light, bake AO textures
  (Bake/*.szs, needs an importer bump), the airship / castle / desert sky variants.
  3D World (references recaptured 2026-10-24, MAE): ground 53.7, underground 34.3, castle 75.9,
  airship 26.9, water 33.3, ghost 31.8, snow 70.6, desert 60.4, sky 24.6, forest 48.4 (were
  31–77 before: the start sign 3× too big — a model without an SMB1 picture is drawn at its own
  size —, Mini Mario's face / eyes / hands misplaced — parts in files of their own are modelled
  in the model's axes —, Mini Mario stretched by Mario's animations — retargeted, its own rest
  translations). Left in 3D World, measured (mean RGB, ref vs ours, 3W ground): the sky top
  (67, 105, 218) vs (204, 213, 253) — our sky-material model is wrong (desert's comes out green);
  the field's dirt too dark (185, 167, 47) vs (140, 112, 51); the far scenery too blue (109, 152,
  146) vs (69, 115, 176) — the distance fogs too strong or the wrong colour. Halving the exposure
  made everything worse (not overexposure). Next: the sky material and the light-map lighting
  from the game's shaders / `bagllmap` data; the ground's texture (pebbled dirt, ours flat
  stripes); the player's turn angle (ours more side-on). Done since the revival: start sign, 2D drop
  shadow (wgpu + Swift), animated blocks / coins (importer 10), night darkness (importer 12, wgpu
  + Swift), SceneDB backdrops for every style and night, night field art, airship sway and sky
  drift. Open: the final colour table (every pixel 1–2 levels off: ramp backdrops ready in
  `/private/tmp/claude-501/pixels/ramp_*`, `research/tools/pixel-ref/ramp.py`), the airship's
  cloud start ground and night lightning / rain phase, SMB3 / SMW sky and airship backdrop
  heights, the castle lava surface (3 px low, wrong frame rate), backdrop actors (UFOs …),
  animation phases, the Swift app's shadow / darkness not checked on frames, NSMBU's background
  lighting (sky too dark), 3D World
- 🔄 Pixels: per style × theme frames against Ryujinx (2D styles exact; NSMBU / 3D World lighting,
  terrain variants, effects)
- 🔄 Sound: every object's sounds, mixer behaviour checked against the game's output · keys
  (`key_get`, `key_appear`), pink coins (the count's note, the last one's chord) and Burners
  (`fire`) ✅; a container's child key is found by name (the 2D styles' pink coins) [inf]; knock-outs
  by what did it (`r_damage_star` / `_shell` / `_tail` / `_quake`, else `r_damage`; the sim records
  the cause), spin-jump stomps (`r_damage_spin`), stomps from Yoshi (`r_damage_ystamp`), a sliding
  shell's knock-outs climbing its combo (`DownComboCount` along the shell's chain), enemies and
  items splashing into and out of the area's liquid (`r_splash_in` / `_out` by `BgWaterType`); the
  Master Sword, Superball Flower and SMB2 Mushroom's own course music, hurry track and fanfares; the
  update power-ups' sounds (sword, shield, arrow, SMB2 pull / throw, balloon, wearables, boomerang);
  the cape's spin and climb, the cat's wall climb, slide down and dive landing; 3D World voices
  for dying, grabbing the pole and throwing fire (`MachinedPlayer` = own player); parachutes opening;
  bosses' jumps and landings where the style has them (NSMBU Bowser; small / large by speed [inf]);
  3D World's dash footsteps ✅

**G9 — Every platform** 🔄
- 🔄 macOS (Swift app) · 🔄 iOS / iPadOS (built for the Simulator; the owner's iPad) · 🔄 Windows ·
  🔄 Linux · 🔄 web · 🔄 Android
- 🔄 Android (`crates/android`, `scripts/build-android.sh`): the portable shell and the SMM2 front end in
  a NativeActivity (no Java code, no Gradle), arm64, API 28+, Vulkan. On-screen controls (d-pad,
  Jump / Run / Spin, +; several fingers, drawn while playing; a touch elsewhere is a tap for the
  menus), the surface dropped and remade across background / foreground. Checked in the Android
  Emulator (API 35, arm64) with an asset pack pushed by `build-android.sh --push`: opening, title,
  Play menu, Course World, the library list, a course page and play by touch. Open: the import on
  the device (G1's NSP importer + a file picker), a real phone, Bluetooth controllers (winit key
  events), the 3D World sky drawn in stripes in the emulator, sound not checked (emulator audio off)
- ⬜ Release packages for every platform (the game only; the player imports their own dump)

---

# History (Phases 1–11, 2026-10-17 → 2026-10-19)

## Roadmap
✅ done · 🔄 in progress · ⬜ not started · ⏳ waiting on owner

### Phase 1 — Environment and extraction ✅
- ✅ Dumps gitignored; provenance checked; toolchain pinned; CI + pre-commit guard
- ✅ SMM1 decrypted (base v16 + update v208); SMM2 v1.0.0 extracted; inventories; 550-course sample
- ✅ SMM2 update v3.0.3 extracted (patched RomFS + ExeFS); both executables converted to ELF

### Phase 2 — Course formats ✅ (awaiting go-ahead)
- ✅ Community format docs gathered and cross-checked (13 sources; disagreements recorded)
- ✅ Confirmed: SMM2 `.bcd` = 0x10 header + AES-CBC body + IV / random state / CMAC; SMM1 `.cdt` = 0x15000 BE, CRC32 @0x08 over [0x10:]
- ✅ Format docs: [docs/formats/smm2-course.md](docs/formats/smm2-course.md), [docs/formats/smm1-course.md](docs/formats/smm1-course.md)
- ✅ `smmfmt` reference package: SMM2 course parser/writer, SMM1 `.cdt` parser/writer (CRC), course codes, `python -m smmfmt` summary CLI
- ✅ SMM2 `.bcd` decrypt/encrypt; key table found at runtime in the owner's executable (`smmfmt.smm2.keyfind`, ~2 s; same table in v1.0.0 and v3.0.3)
- ✅ Byte-exact round trips: 550 dataset courses, 180 built-in SMM2 courses × 2 versions (decrypt → parse → write → re-encrypt), 156 built-in SMM1 `.cdt` — 30 tests, CI runs the 24 that need no game data
- ✅ Finding: path arrays (snake, clear pipe, creeper, ! block, track block) are slot-indexed and can be sparse
- ⏳ Owner's own saved courses (optional; not needed to proceed)

### Phase 3 — Binary analysis ✅ (awaiting go-ahead)
Specs are private (`research/specs/`, gitignored): behaviour in prose/tables, no decompiled code.
- ✅ Ghidra headless analysis of SMM2 v1.0.0 and v3.0.3 (≈7–8 min each), SMM1 `Block.rpx` v16 + v208
  (own RPX→ELF converter, nothing installed); query scripts in `tools/ghidra/`
- ✅ Data-file survey: physics is in code; EnemyDB attack matrix, EditDB parts, per-style SceneDB,
  263 player states are data; 108 lesson input replays found
- ✅ Course-loading pipeline: decrypt → validate → placements → actors; part→actor tables per style;
  flag bits resolved (big = 0x4000 size field, 0x400 = on track, 0x1 from pipe, …)
- ✅ Player physics: one shared model for all styles (style = feature switches); SMB1 constants
  complete (walk 1.5 / run 3.0, jump 3.628 + speed bonus, held-jump gravity bands, fall cap −4.0);
  simulated jump heights 4.2 / 5.4 tiles match the game; unchanged in v3.0.3
- ✅ Themes: night = area flag bit 1; per-theme day/night effects, wind, liquids, autoscroll, timer,
  clear conditions (CRC32 of internal names)
- ✅ Objects: 82 object specs + shared mechanics (actor base, 18 enemy contexts, generic walker,
  attack/reaction matrix, blocks); all top-25 objects in depth
- ✅ SMM1 100 Mario Challenge code: 11/14 open questions answered (requests 50 courses per band,
  lives cap 100, ≤3 banked lives per course, free unlimited skip, unlock rules, reward selection)
- ⬜ Left for later phases: enemy stomp bounce speed, activation range, 44 style power-up and 42 clown
  car specs, remaining 51 rarer/2.0+ object ids (Phase 7)

### Phase 4 — Engine architecture ✅ (awaiting go-ahead)
- ✅ Options with trade-offs: [docs/architecture-options.md](docs/architecture-options.md)
- ✅ Owner chose **B: Rust core + thin Swift shell**
- ✅ Working design: [docs/architecture.md](docs/architecture.md) — crates, threads, determinism rules,
  FFI shape, importer pipeline (RomFS/ExeFS folder in, Metal-ready cache out), placeholder pack,
  testing, dependencies, Phase 5 order of work
- ✅ Asset facts checked on the dump: textures ~98 % RGBA8 sRGB + BC1/4/5/7 (all native on Apple
  Silicon, need Tegra unswizzle); 2D styles are BFRES quads; audio DSP-ADPCM

### Phase 5 — Vertical slice (SMB1, ground) 🔄
- ✅ 1. Rust workspace (formats, assets, import, sim, ffi, cli) + Swift/Metal app; CI on macOS + Linux
- ✅ 2. Importer: identifies the game build, finds the course key table, converts tables, exports
  M1 textures (RGBA8 + BCn, Tegra unswizzle) into a Metal-ready cache outside the repo
- ✅ 3. Rendering: tiles, sprites, letterboxed integer scaling; theme backgrounds (M1_DV_*, day/night,
  parallax 0.5 [inf]) with the clear colour from the background's sky (black underground/castle); BFRES reader (0.9: skeleton,
  materials, shapes, texture-pattern animations) → importer `sprites.json` (1952 of 1963 M1
  textures placed by their model quad, all animation frame keys) → FFI draws model sprites at the
  sim's origin + quad. Hand offsets gone; Goomba walk animation fixed (it never animated) and
  timed from the data (10 frames per texture); goal flag mirrored as in the game. Player walk-rate
  scaling and the other objects' frame timings are measured but not wired in yet
- ✅ 4. Audio: BWAV/BARS parsers + DSP-ADPCM/PCM16 decode; importer `audio` stage (22 SMB1 sounds:
  jump, coin, stomp, kick, bump, break, item, power-up/down, 1-UP, fireball, pole, time count,
  death / clear / hurry fanfares, ground + hurry + star music) into `<cache>/audio/`; `mmk-audio`
  mixer (32 voices, loops, music chaining, fades, exclusive groups, linear resampling to 48 kHz,
  master/music/SFX volumes); engine maps sim events to sounds (hurry jingle → fast music, star
  music, death and clear fanfares, count-down loop); AVAudioEngine output (`MMK_MUTE=1` to
  silence); `mmk audio` CLI lists/decodes sounds to WAV · other themes' music, night music and
  object sounds beyond the slice ✅ (Phase 7 "Gameplay sound from the game's own sound links")
- ✅ 5. Terrain: ground, blocks, pipes, semisolids, slopes ✅ · ice ✅ · in-course camera ✅ (4 px
  dead-zone follow, filtered vertical tracking, six vertical speed strategies, low-camera 216 px
  ceiling, full-height scroll-stop walls, course start, frozen on death, goal); autoscroll and
  vertical-area camera come with those features
- ✅ 6. Core objects: coins; ? / brick / hidden / hard / note blocks (bump curve, contents, multi-coin,
  breaking + debris); Super Mushroom, Fire Flower + fireballs, Star, 1-Up; Goomba (+ Paragoomba),
  Koopa green/red (+ Paratroopas), shells (kick, slide, chain kills, wake-up); stomp chains, damage
  (Fire → Super → Small), invincibility, death sequence, restart, lives; course timer + hurry;
  M1 goal pole (height score, slide, walk to castle, time → score countdown, castle flag)
- 🔄 Navigation: warp pipes in all four directions (entry windows, align / 0.75 px/f travel, 30 + 30
  frame holds with fade), main ↔ sub-area with per-area state kept, doors (same-area pairs)
- 🔄 More objects: Piranha Plants (free / pipe / fire), pipe generators, big coins, checkpoints
  (respawn, big coins kept), Thwomp (+ side), lifts (static / falling), springs (normal / held high
  bounce), Bill Blasters + Bullet Bills, spike traps; moving-solid layer with rider carry; P switch
  (brick ↔ coin swap, P doors), ON/OFF switch + dotted-line blocks, falling donuts, Munchers,
  conveyor belts; Spiny, Buzzy Beetle (+ fire-proof shell), Spike Top (crawls floors, walls, ceilings), Hammer Bro +
  hammers, Bob-omb (fuse, kick, explosion), Dry Bones (collapse/reassemble), Boo; parachutes,
  one-way walls, Burners, Fire Bars, Banzai Bills; vertical areas; liquids (static / one-way /
  oscillating water, lava, poison; deadly kinds kill) and swimming (strokes, sink, swim table,
  jump-out window; underwater theme, castle-night air swimming); seesaws (rider torque, tilt
  limits), Bumpers, Grinders (static), airship cannons + cannonballs; Lakitu (+ Spiny eggs) and
  rideable Lakitu clouds, POW quake, keys + locked doors; snake blocks (path codes decoded from
  the corpus, multi-segment moving solids, fall-apart), Twisters, swinging claws (grab/release),
  Cheep Cheeps (swim / flop), Bloopers (pulse + sink), Skewers, Magikoopa (+ magic), Rocky Wrench
  (+ wrenches), Wiggler (segments, anger), lava lifts; preset autoscroll (push / crush); night
  effects: sky-night low gravity, desert-night wind, underground-night upside-down view + swapped
  Up/Down, snow-night ice, forest-night poison, castle-night air swimming
- 🔄 Coverage (`mmk coverage`): 33 of 150 sample SMB1 courses fully playable (was 20); biggest
  blockers now Chain Chomp, tracks, Bowser, clear conditions, swinging claw, grinder, liquids
- 🔄 7. Tune against Ryujinx ✅ frame-exact loop: a locally patched Ryujinx (1.3.2 + a trace hook on
  the player's per-frame update, scripted pad state per emulated frame, private data root) runs
  the owner's SMM2 v3.0.3 on physics test courses (`mmk trace-courses`, played as title courses);
  `mmk trace` runs the same input script in the sim and the two are diffed frame by frame (tooling
  private). 454 recorded tests over all five styles (walk/run/skid, every jump height, air turns,
  jump chain, crouch/duck/crouch jump, spin and 3DW long jump, ground pound, wall slide/jump,
  ledges, slopes, swimming, Super Mario, style power-ups): 220 matched exactly before, 428 now
  (landing frame, jump-press retry, CrouchEnd, 3DW run timer, slope geometry, wall
  slide only in NSMBU/3DW, ledge fall, …) · ⬜ power-up specials (cape glide/flight, raccoon
  flight, propeller, cat attack), slide end on flat, pit-death height, enemies, real courses
  start to goal

### Phase 5b — Native app (must be done before Phase 6) 🔄
- ✅ `.app` bundle: `scripts/make-app.sh` → `build/MMK.app` (Info.plist, generated placeholder icon,
  ad-hoc signature); working name "MMK" kept in one file (`Identity.swift`)
- ✅ First-launch import screen: RomFS/ExeFS (+ optional update) pickers, importer on a background
  thread with progress (`mmk_import_run`, update RomFS overlaid via symlinks); "use placeholder art"
  fallback draws coloured boxes per sprite layer (`MMK_TEXTURE_NONE`)
- ✅ Course library (home): scans the library folder, name/style/theme/timer via `mmk_course_info`,
  search, filter, sort, add files/folders (deduplicated by content hash), double-click to play
- ✅ Pause (Esc/P/+): Continue, Restart (`mmk_engine_restart`), Exit; course clear and game over
  overlays → library; menus work with keyboard, controller and mouse
- ✅ Settings window: keyboard + controller remapping, full screen, integer scaling, volume sliders
  (stored for the audio engine: `volume.master/music/sfx`)
- ✅ Save data: per course (content hash) cleared flag, best time, clears, attempts in `saves.json`
- ✅ "Playable" / "Not yet supported" marking (style M1 for now; `unsupported` object count hook)
- ✅ Built-in courses from the owner's dump in the library: the importer exports the 120 Story Mode,
  50 lesson and 10 title courses (encrypted, decrypted with the cached key table); "Built-in" filter
- ⬜ Cancel button for a running import; incremental re-import
- 🔄 SMM1 importer + layout player started: `mmk import-smm1` converts the owner's Wii U UI
  (337 layouts, 1698 animations, 2576 textures, 9 fonts, English text) into a cache; the `ui`
  crate plays layouts + animations into quads; the app draws them (`UILayer`, debug view
  `MMK_UI_LAYOUT`) — world map, course intro, course clear, game over, title menus verified
- 🔄 Shared menu look with the SMM1 layout player (Phase 6): 100 Mario screens done

### Phase 6 — 100 Mario Challenge 🔄 (owner: SMM1 presentation like-for-like)
- 🔄 Research: SMM1 100 Mario screen sequence from Block.rpx v208 + layout data — scene → layout
  map done (private `research/specs/smm1-layouts.md`: title, main menu, Course World, difficulty
  select, world maps + Mario/course/castle parts, course intro, skip, clear, pause, game over,
  ending text, credits, costume reward); still open: the rescue scene itself (not a layout), the
  run-flow timing between screens
- 🔄 SMM1 asset import (owner's disc dump): BFLYT/BFLAN layouts ✅, GX2 textures (BFLIM, AddrLib
  1D/2D untiling) ✅, BFFNT fonts ✅, MSBT text (English) ✅, BFSTM/BARS audio ⬜
- 🔄 Layout player (panes, text, keyframed animations) — pane tree, parts, pictures, windows,
  text with font fallback, SRT/visibility/alpha/colour/texture-pattern/texture-SRT curves ✅;
  part property overrides: picture material from the enclosing layout (course-dot numbers,
  castle variants) and basic values (visibility, translate, size, scale, rotate, alpha; import
  now keeps them) ✅; "influenced alpha" fades the whole subtree incl. parts ✅; children without
  anchor bits sit on the parent's origin point [inf] ✅; caller visibility, pane query, message
  templates with value placeholders (FFI `mmk_ui_set_visible/pane/message`) ✅;
  per-character transforms (GAME OVER letters), alpha compare, indirect textures ⬜
- ✅ Mode logic (`core/crates/modes`): run lengths 8/16/16/6 (10 Mario 8), 100 lives = cap, ≤3
  banked extra lives paid on clear (+1 refund in 100 Mario), death retries from the course-start
  state, free skip (100 Mario only), Save & Quit, game over, completion; course pools by
  difficulty band with SMM1's shuffle and castle search; Continue records, unlock rules, stats ·
  costume reward selection ⬜ (category table not read)
- ✅ Run session (`modes::challenge`) + FFI (`mmk_challenge_*`, `mmk_engine_set_run_counters`):
  New Game / Continue, life pre-charge autosave, deaths retry with the course-start counters,
  1-Ups and coin rollovers banked through the sim's `lives_gained` (HUD lives stay put), skip,
  Save & Quit, unloadable-course replacement, unlock announcement once, credits list
- ✅ Course pool: local library (difficulty value = fail rate from the dataset's `index.json`
  clear rate, else the SMM2 bucket); castle-theme courses preferred for the last course; no
  repeats within a run while others remain · remote source ⬜
- ✅ Pool limited to courses marked playable; grows with Phase 7
- ✅ App flow (functional stand-in screens): difficulty select with locks and Continue, course
  intro card, run banner, pause menu Skip / Start over (−1 life) / Save & Quit, game over, run
  cleared + unlock + course credits; `hundred.json` save (kept as the fallback without the
  SMM1 import)
- ✅ SMM1 layout screens (`ChallengeScreens.swift`, plain classes stepped at 60 Hz; screenshots
  via `MMK_UI_SCREEN=challenge*`):
  - ✅ difficulty tabs `Nwk_ListTitle_00` over the selected `Nwk_WorldMap*_00` (pre-MapIn view),
    New Game / Continue, lives and n/total from the Continue record, locked tiers greyed
    (Invalid) — the game hides them instead [obs data], see DECISIONS
  - ✅ world map: MapIn, Goombas carry Peach under the banner (Telop + MoveKuribo), Peach on the
    castle (AnimB), Mario jumps out and walks (MoveMario, stops found from the dot positions),
    cleared dots (Type) with the Clear pop, Mario walks to the next dot after each clear, Out
  - ✅ intro card `Play_CourseIn100Mario_00`: name, maker, SMM2 code (from the dataset id), lives,
    n/total, one Mario per life; AnimA → AnimB → hold; after a death AnimB + OutMiss over the game
  - ✅ in-course overlays drawn by the game Renderer: `Cmn_PauseMenu_00` (mode line
    "100 Mario Challenge (n/total)", Start Over, Save & Quit, close), `Play_CourseClear_00`
    (times incl. the dataset's world record, Save & Quit / Next), skip countdown
    `Play_SlidePassIconKey_00` (hold − / Tab / controller −, 3 × 32 frames, release cancels)
  - ✅ `Cmn_GameOver_00`; ending: `Play_EndingText_00` over the finished map (Easy: another
    castle; Normal; Expert/Super Expert) then `Nwk_StaffCredit_01/00/03` with the cleared courses
  - 🔄 timings between screens, the rescue scene (not a layout), Mii/flag/Star/comment areas
    (hidden: no data), per-character GAME OVER letters, UI sounds ⬜
- ⬜ SMM1 → SMM2 course import design + documented losses
- ⬜ App screens drawn with the SMM1 layouts (world map walk, intro card, clear, game over, ending)

### Phase 7 — Expansion ⬜
- ✅ Styles: all five styles' art imported (importer v4); SMB3 and SMW courses play with their own
  art (SMB1 fallback per model/frame); carrying shells, slope slide (not SMB1), SMW spin jump (new
  Spin button: C / R / ZR); the sim owns the playable-style gate — sample 319/550 (M1 148, M3 94,
  MW 77) · Yoshi, cape dive flight ⬜
- 🔄 NSMBU / 3D World rendering (3D mesh renderer; both styles playable since the merge —
  sample NSMBU 99/100, 3D World 21/100, the rest blocked on 3D World-only parts):
  - ✅ BFRES reader: shapes with index buffers per LOD and skinning attributes, skeleton matrix
    palette + inverse bind matrices, skeletal animations (cubic / linear / step / baked curves,
    wrap modes), bone visibility animations; real files checked in tests
  - ✅ Importer v5 mesh stage: 354 `meshes/*.mmesh`, 3500 `anims/*/*.mskel` (all WU_/3W_ models,
    StaticModel.pack included) in ~12 s total import
  - ✅ FFI: mesh data, animation lookup, posing (`mmk_mesh_*`), per-frame mesh draws from the sim's
    sprites (`mmk_engine_meshes`); Metal path with GPU skinning (`MeshRenderer.swift`), model
    viewer `MMK_MESH_PREVIEW`
  - ✅ Drawn: 3D World Goomba, Koopa (+ shell), Mario (body, face, eyes, hands; small, super,
    Fire), blocks, coins, items, terrain as per-cell meshes; NSMBU Mario (body + head) and the
    pre-rendered enemy / item / object cards; NSMBU terrain stays the atlas
  - ✅ Reference frames from the owner's SMM2 in Ryujinx (private root, LayeredFS title-course
    replacement, in-process key / grab dylib; scripts outside git) and `mmk-play --size
    --view-at` diffs: 24 dataset courses (WU all themes but water + nights, 3W all themes),
    mean MAE 61.1 → 49.4 (8× blurred 58.2 → 45.2)
  - ✅ Game cameras from the binary: 3D World field perspective (fovy 12.5°, raised 35 px,
    looking 5 px up), NSMBU orthographic; background scenes (`_DV_` models) with the DV camera
    from SceneDB (`DV_Play_Fovy`, ProjOffsetY / ProjMoveY / CamMoveY), own depth pass; the 2D
    backdrop gives way to them
  - ✅ Environment lighting / fog per theme (`env/*.json` from Env/Cmn(_DV).szs, AAMP reader),
    materials: blend / alpha test / faces, self-lit, emission, alpha masks, sky palette,
    world-space texture coordinates (castle walls), TEV tint, texture SRT, vertex colours;
    WU tiles use whole atlas cells (UVs were a quarter cell); big actors' scale
  - ⬜ Per-actor animation clocks and transitions (time = world tick), 3D World neighbour variants
    for terrain edges, pipes / note blocks as models, small 3D World Mario's cap; the game's
    light maps (runtime cube maps) and distance-fog formula (3W snow / airship too foggy), WU
    background tints, liquids (lava / water as the 2D sprite), normal / specular / bake maps,
    bloom and colour correction, the sim's start camera in normal-boundary courses (~30 px high)
- ✅ Super Leaf (Raccoon: tail swing hits enemies/blocks, P-meter 63 frames → 240-frame flight,
  flaps vy 1.5 × 18 f, slow fall ≥ −1 × 15 f) and Cape Feather (cape spin both sides, glide ≥ −1,
  simplified meter launch); goal-as-Raccoon / Cape conditions
- ✅ Clear conditions: header hash → condition; defeat / collect / coins / POW / P Switch counts
  (each placed object once), goal as Super / Fire / invincible, no damage / no landing; the goal
  lets the player pass until met; HUD shows progress · holding / riding conditions ⬜ (no carry,
  Yoshi, shoe yet) — SMB1 sample 53 → 65 playable
- ✅ Chain Chomp (chained + unchained)
- ✅ Koopa Clown Car (and Fire car): floats empty, chases with an enemy rider (riders linked by
  link id, carried on top), flown by the player (2 px/f, 0.05 accel, Fire shoots, Down+Jump exits
  [inf]); "goal in a Clown Car" condition
- ✅ Winged / tracked blocks (brick, ?, hard, ice, cloud) as moving solid actors that release
  their contents when bumped; winged lifts bob; winged Thwomps patrol and bob (12-thwomp.md)
- ✅ Fish Bone: patrol, 40-frame aim, charge (2 / 6 px/f), shatters on contact; fire-proof
- ✅ Custom autoscroll: camera centre follows the area's part-89 nodes (speed per segment,
  0.03 easing, pauses during warps/death/goal; the player is pushed at the edges); the main area's
  path starts at the start unit and ends at the goal unit, so a course without nodes still
  scrolls (binary pass 3)
- ✅ Binary pass 3 (course loading): slopes as caps + diagonal (flush with the ground), spike
  block colours (0 red / 1 blue ON/OFF, 2 timer), cannon facing + aim, goal pole over the goal
  ground's first column, angry alternate Wiggler
- ✅ Conveyor slopes (steep slope + belt carry); winged/tracked coins and note blocks —
  SMB1 sample 148/150 playable (left: a Bill Blaster and a Burner on tracks)
- ✅ Theme music per style: the importer copies every style's theme / hurry / star tracks (105
  streams); the player decodes only the course's tracks into per-course mixer slots · per-style
  sound effects ✅ (style variants of SFX and fanfares, SMB1 fallback), sub-area music ✅
- 🔄 Gameplay sound from the game's own sound links (DECISIONS.md "Gameplay sound from the
  game's own sound links"; private `research/specs/audio.md` §5–§11, `audio-triggers.md`):
  - ✅ formats: SLink2 users with containers (switch / random / blend / sequence), conditions,
    random ranges, property curves, user params, action triggers; the group tree (AGST),
    attenuation sets (AATN / AROC / AACL)
  - ✅ import v7: `audio/sound.json` (616 gameplay users, groups, attenuation, per-theme
    effects, ground materials) + 2430 effect waves (`audio/se/`) + 234 streams
    (`audio/music/`, game names); `mmk import-sound <romfs> <cache>` runs only this stage
  - ✅ mixer: per-voice volume / pitch / pan / delay / duration / fade-in / low-pass, group
    voice limits with priority stealing and folder limits, ducking, theme echo + reverb sends,
    four-channel music with a fade-in layer (Yoshi drums), DSP-ADPCM music decoded while
    playing, music position clock, final limiter
  - ✅ director (host `sound.rs`): keys per style / theme / day-night / object; player events
    (jump by form, triple-jump count, swim, spin, wall kick, power-up / item / damage, pipe,
    door, head bump, pole, checkpoint, clear condition), player states (landing, splash in /
    out, ground pound, vines, shoes, Yoshi, skid, propeller, footsteps by ground material),
    objects (coins incl. 10/30/50, blocks, ON/OFF, P switch, 1-Up incl. 100 coins), actors
    from their state changes (stomp with combo / big / wings, defeat, Koopa into shell, kick,
    shell wall hit, Thwomp landing, Bill Blaster shot, out of a block); music: area theme
    track (day / night, 3D World day only), hurry jingle → hurry track, Super Star / P switch
    tracks with the theme track resumed at its position, star end sign at 120 frames, death /
    clear / castle clear fanfares, sub-area music and effects, theme ambience (NSMBU / 3D
    World), underwater filter for sounds below the water surface, 3D World distance
    attenuation and culling, pan by screen position [inf]
  - ✅ `mmk audio-render <course> --cache DIR --out x.wav [--log] [--only sfx|music]`:
    headless scripted run through the same director and mixer; a rendered SMB1 jump matches
    the game's wave exactly (correlation 1.000)
  - ⬜ springs, note blocks (pitch by height), POW, Bob-omb, Burners, cannons, projectiles,
    bosses' actions, keys, pink coins, objects' splashes and parachutes, star / shell / spin /
    Yoshi kills, cape / tail / cat / hammer actions, 3D World voices, Yoshi's own sounds,
    costume music; checks against the running game (pan, limits, effect sends)
- ✅ Wall slide (≥ −2.0) and wall jump (vy 3.5, 2.25 away) in every style; NSMBU/3DW jump chain
  (third jump ×1.05, 10-frame window [inf]) and ground pound (wind-up, −6.0 dive, hits blocks and
  enemies below)
- ✅ Propeller Mushroom (NSMBU style power-up): Spin rise toward 2.5 for 50 f, slow fall −1.25,
  drill −6; goal-as-Propeller (ready for when NSMBU renders)
- ✅ 3D World blinking blocks (two groups, 240-frame swap) and spike blocks (shared timer: warning
  118–177, out 178–239; ON/OFF-linked modes [inf]) — placeholder tile art until 3W models draw
- ✅ Yoshi (SMW / NSMBU, yoshi.md): egg hatches [inf: 30 frames], mount from above, riding with the
  on-foot tables, Spin dismount per style, NSMBU flutter, tongue (eat / hold & spit shells, fire,
  bones; power-ups to the rider [inf]), Red Yoshi fire (60-frame cooldown), stomps kill spiny
  enemies, hit → rider pops up and Yoshi runs off; goal-on-Yoshi condition — SMW sample 97/100
- ✅ Pink coins: counted once each across both areas; the last one gives the key
- ✅ Goomba's Shoe / Stiletto: shoe Goomba hops (50-frame pause, 4.5 / 1.5), leaves the empty
  shoe; the player wears it (spikes and Munchers underfoot safe, shoe stomps, a hit knocks it
  off); "goal in a shoe" condition · shoe ground-pound, Yoshi (SMW/NSMBU) ⬜ — SMB1 sample 93/150
- ✅ Vines: climb areas with tile art from the course data; climbing (1.0 / 1.5 px/f, 6 px
  hang, side switch, jump off with 30-frame lock) · vines sprouting from blocks ⬜

- ✅ Bowser: 20 HP (big 40; fire 1, shell 3; star / POW / lava kill), ±64 px patrol facing the
  player, middle jumps and SMB1 Fly, fire every 40–79 frames + breath with level lane flames; SMB3
  big jump + hip drop; key drop · hip-drop quake, water timers, helper modes ⬜
- ✅ Boom Boom: 3 HP + fireball counter 5 / 10, run 0.75 / 0.75 / 1.5, wall jumps (3 per side),
  charge → Defend spin, DamageDown 70, winged cruise + swoop; Pom Pom (3D World alternate) [inf]
- ✅ Bowser Jr.: 3 HP + fireball counter, wait / walk ±24 / fire / hammer / hop / shift jump →
  hover → hip drop, DamageDown → shell slide; Clown Car riders for all three [inf] — SMB1 sample
  77 → 97 playable
- ✅ Bosses pass 3 (from the binary): shell slide / wakeup, Clown Car steering + ride heights,
  Jr.'s clown damage and thrown Koopas, Pom Pom's throw / hide / Lapse + homing shuriken, NSMBU
  anim lengths, boss Key → key event · Bob-omb blast type, Pom Pom decoys ⬜
- ✅ Lava Bubble (hide / cosine rise / fall), Monty Mole (buried, mound, leap, chase), Angry Sun
  (corner wait, spin, half-ellipse swoop) and Moon (wipes on-screen enemies)
- ✅ Tracks: graph from track records (lines + quarter arcs), rail follower (0.75 / 1.5 px/f),
  any actor rides (lifts, seesaws, grinders, claws, items, enemies); drawn as lines · junctions,
  open ends, blocks/coins on tracks, exact track tiles ⬜ — SMB1 sample 49 → 52 playable
  - ✅ Pass 3: the game's rail grid (record templates, end words, cell words) and its follower
    (cell-centre walk, 2×2 bends at r 24, closed / open ends, start field), junctions 8–15
    switched by ON/OFF — all 657 corpus riders attach, every fall is at an open end · re-catch
    of a falling rider onto a lower track (0x7100343680) ⬜
- ⬜ Measure object/style/theme frequency in real courses
- ⬜ Add styles, themes, objects in that order (2.0/3.0 items need the update dump)

### Phase 8 — Every platform: macOS, iOS, Windows, Linux, web 🔄 (owner request 2026-10-18)
Approach (DECISIONS.md 2026-10-18, "Cross-platform shell"): the Rust core stays platform-neutral;
a portable Rust shell runs the game everywhere, the Swift app stays as the Apple-native shell.
- 🔄 `shell` crate (`mmk-play`, docs/architecture.md §5b): winit window + fixed 60 Hz loop, wgpu
  renderer (sprites, BFRES quads, layers, flips, sky colour, upside-down view, placeholder boxes;
  BCn on the GPU or CPU-decoded incl. our own BC7 decoder), keyboard + gilrs gamepads, cpal audio
  via the shared sound director, minimal HUD (built-in 5×7 font), pause / clear / game over,
  `--screenshot` headless smoke test. Shared front-end code moved to the new `host` crate (used by
  `ffi` too). Verified on macOS (M4 Pro, Metal): offscreen frames pixel-identical to the Swift app
  below the HUD; `--check-textures` agrees with the GPU on every BC texture. Layouts ✅: the
  layout player's quads drawn by `ui_layer` (port of `UILayer.swift`: two-texture combine, black /
  white colours, per-quad blend and sampler); `mmk-play --layout NAME --ui-cache DIR [--ui-anim]
  [--ui-text] --screenshot` matches the Swift app's MMK_UI_LAYOUT preview (SMM1 Cmn_GameOver_00,
  SMM2 Play_Title_00 under three animation specs: ≤ 4 px off by ≤ 3 levels). Open: the SMM font
  HUD, clear-condition HUD text, menus/library (presentation flow lives in Swift today)
- ✅ 3D models in the portable shell: the mesh planner moved to `mmk-host::mesh` (shared with
  the Swift app); `mmk-play` skins on the CPU (works on every wgpu backend, WebGL2 included) and
  draws meshes with a depth buffer, interleaved with sprites by layer like the Swift renderer;
  `mmk-play --map` renders whole courses, `mmk gallery` / `mmk missing-art` check every part's art
- 🔄 Windows and Linux builds of the shell; CI jobs on Windows + Ubuntu (clippy, tests, release
  binaries) and wasm32 on Ubuntu. Windows type-checks locally (clippy, MSVC target); not yet run
  on Windows or Linux hardware
- 🔄 Web: wasm32 build of core + shell in a canvas (WebGPU, WebGL2 fallback), `scripts/build-web.sh`.
  Playable with the owner's own data (docs/architecture.md §5b "Web"):
  - ✅ Import game data from a picked folder into OPFS (never uploaded, kept between visits):
    an `mmk import` cache is copied (~15 s), or the dump's `romfs/` + `exefs/` is imported in
    the page by `mmk-import` itself in a worker (~35 s, ~1.5 GB peak; output identical to the
    desktop import); textures stream from storage on first use, music per course
  - ✅ WebAudio (cpal) through the shared sound director, started on the first gesture
  - ✅ Gamepad API (gilrs) and keyboard; course picker: built-in courses of the import, or any
    `.bcd` / zlib / raw course file
  - ✅ SMM2 menus (`mmk-front`) when the SMM2 UI import is in storage: copied from an
    `mmk import-smm2-ui` folder, or made by the in-page dump import (identical to the desktop
    import, all 4528 files); the UI cache reads JSON up front (13 MB, ~0.4 s) and textures on
    request; opening, title over its demo courses, Play menu, Course World (the library: built-in
    courses + opened files), Story Mode, 100 Mario, course card / HUD / pause / give up; UI sounds,
    course music, keys, mouse clicks; saves in `localStorage`. No server listing in the browser
  - checked in headless Edge (CDP: import, courses, keys, a scripted Gamepad API pad, non-silent
    WebAudio, the menus) and headless Firefox (WebDriver BiDi; before the menus). Open: Safari
    (OPFS `createWritable`), a real controller in a browser, touch controls, mobile memory
    limits, the course server from the browser (HTTP), Firefox with the menus
- 🔄 iOS/iPadOS: the same Swift package cross-compiled (`swift build --triple`, core linked from
  `core/target/aarch64-apple-ios{,-sim}/release`), bundled by `scripts/make-ios-app.sh`
  (DECISIONS.md "iOS packaging"); `scripts/check.sh --ios` builds it for the Simulator. AppKit
  bits split behind `#if os(macOS)`; on iOS: UIKit MTKView hosts, touch controls (d-pad, Jump /
  Run / Spin, + / −; multitouch, sliding between buttons, hidden while a controller is
  connected), hardware keyboards through `GCKeyboard` with the same bindings, controllers as on
  macOS, data folder = Documents (shown in Files: the owner copies a cache made on the Mac into
  `assets/`, or picks one and the app copies it in), Files picker for courses, settings sheet,
  compact course list on iPhone. GPUs without BC support get BCn decoded by the core
  (`mmk_texture_decode_rgba`; BC7 decoder moved from `shell` to `formats`). Verified on the
  Simulator (iPad Pro 11" and iPhone 18 Pro, iOS 27): placeholder art, the real cache (found
  through the Mac's home), CPU texture path, title screen, library. Open: signing + a run on the
  owner's iPad (`ios-install-tailscale <app> --device ipad` once signed), tap-to-select in layout
  menus, iCloud placeholder files in picked folders, the in-app importer on iOS
- 🔄 Engine-drawn menus everywhere (layout player), so every platform shows the same screens:
  ✅ the SMM2 front end is Rust (`mmk-front`: opening, fades, title + Play menu, Course World over
  a library folder and the course server, Endless / SMM1-layout 100 Mario, in-course HUD / card /
  pause / settings / clear / give up / time up / game over, UI sounds) and `mmk-play` runs it by
  default (Windows, Linux, macOS); headless captures (`--front-screen`) match the Swift app's to
  within rounding on every deterministic screen. ✅ the Swift app runs `mmk-front` too, through
  the `mmk_front_*` C API (DECISIONS.md "The Swift app runs the Rust front end"): its own screen
  classes (~4 500 lines) are deleted; it keeps its renderer, audio, input, touch controls and the
  SwiftUI library / Settings. Its DebugCaptures before vs after on the 30 front-end screens of the
  comparison set (macOS, 1024×640 window): 27 pixel-identical, course-pause 0.11 % and
  course-settings 0.11 % of pixels (the selection frame's Loop phase), smm1-walk 0.35 % (the
  random course name on the map's balloon); real-time navigation (hub → list → course page →
  play) and the `MMK_SCREENSHOT` smoke test run; `scripts/check.sh --ios` builds. ✅ the web build runs
  it too (UI cache over an in-memory file source filled from OPFS, textures on request). Open:
  Settings screens, ghosts in the Apple app, a server listing in the browser (HTTP fetch)

### Phase 9 — SMM2 single-player like-for-like 🔄 (owner request 2026-10-18)
- 🔄 SMM2 layouts through the same layout player (private map: `research/specs/smm2-layouts.md`):
  - ✅ formats: Switch BFLYT/BFLAN 9.0 (all 531 layouts, 2328 animations parse), BNTX textures
    (all 3249 incl. R8/RG8/RGB5A1 → RGBA8), Switch BFFNT 4.1 (BNTX sheets), BFCPX font
    combinations, BFTTF/BFOTF scalable fonts
  - ✅ import `mmk import-smm2-ui <romfs> <out> [--exefs]` → `~/smm-work/cache/smm2-ui-3.0.3/`
    (same cache form as the SMM1 UI; scalable fonts rasterised into bitmap fonts with ab_glyph;
    console button icons drawn; the title courses copied)
  - ✅ player: `<Part>-<Pane>` message labels, part clones, caller pane overrides, global pane
    rectangles, "one material for all frames" windows, colour-only part material overrides [inf]
  - ✅ title screen at app launch (when the import exists) over a scripted demo of the playable
    title courses: prompt → Make (coming soon) / Play → Story Mode (library, Built-in filter),
    Course World (SMM2 screens, below), Coursebot (library), 100 Mario Challenge (extra button);
    game cursor, logo colour by course; keyboard + controller; screenshots `MMK_UI_SCREEN=title*`.
    From the binary (private `research/specs/smm2-frontend.md`): the layout graph, the cursor on
    Make at the prompt, on Story Mode when the Play menu opens, on Play after going back
  - ✅ SMM2 front end from the game's scene code (v3.0.3, private notes above):
    - ✅ opening `Boot_BootScene_00` at launch: the boot page's choice of opening (update
      openings first, then 66 / 8 / 8 / 2 / 8 / 4 / 4 %), InMain, then the black circle into the
      title (Settings > Game, `MMK_BOOT`, `MMK_BOOT_OPENING`)
    - ✅ scene fades `Cmn_Fade_00` (black 30 + 30 for scene changes without parameters, black
      circle from the opening; white, blocks available) over every screen (`SceneFade`)
    - ✅ Course World: hub `Nwk_Title_00` (background loops, Courses / Leaderboards / Endless
      Challenge / the player card; Network Play, Ninji Speedruns, Super Worlds say they need
      Nintendo's network), course list `Nwk_CourseListGeneral_00` (Hot / Popular / New tabs, rows
      with style logo, maker, likes, plays, tags, date, clear mark; open row with description,
      world record, clears / attempts, course ID, Play, and for server courses More info / View
      comments) over the library (dataset index statistics) and the open-source course server
      (Settings > Game; `mmk_online_*`); the online screens of Phase 10 in `mmk-front`
    - ✅ clicks / taps (`FrontInput::click`) on the title, the hub, the course list (tabs, rows,
      an open row's buttons), the online screens, the Endless menu and map, and the SMM1 100
      Mario screens; text entry (`FrontInput::text`) for names, tokens and comments
    - ✅ 100 Mario Challenge in SMM2's Endless Challenge screens (default; SMM1's kept in
      Settings > Game): `Nwk_EndlessModeMenu_00` difficulty tabs (SMM1 unlocks), New Game /
      Continue, the map `Nwk_WorldMap_00` (ChangeMap, Telop, Mario out of the castle, walks
      between dots, InClear, theme icons, Play balloon, his jump), course card and HUD with lives,
      pause Start Over / Quit, clear Quit / Next, game over, the skip countdown
      `Play_SlidePassIconKey_00`
    - ✅ library / Course World / Story Mode play without lives (owner's decision): no game over,
      the death iris, HUD Type 0, card AnimDefault
    - ✅ pause Settings `Cmn_OptionWindow_00` (offline variant): character Mario / Luigi / Toad /
      Toadette (`mmk_engine_set_character`)
    - ✅ menu sounds from the game's data (import v4: SLink2 users + `Param/Layout` sound types →
      `ui/sound/`): buttons, animations (the opening's sound sequence and title call, the course
      card jingle, the pause jingle per style, the Endless map's Telop / dots / Mario)
    - ⬜ Coursebot (`Lcl_*`), Story Mode map (`Qst_*`), Make, the main menu `Cmn_MainMenu_00`
      (the editor's), SMM2's own Endless Challenge rules (endless run, SMM2 lives), Leaderboards /
      Ninji / Super Worlds over the course server, layout scissor clipping, CJK glyphs in course
      names, the pause blur / clear capture effects, menus in `mmk-play`
  - ✅ in-course screens over library courses (`SMM2Course.swift`, DECISIONS.md "SMM2 in-course
    screens"): HUD `Play_PlayScene_00` per style skin (lives + character icon, coins, score,
    timer with the 100 s pulse, clear-condition icon + counter / failed mark, pink-coin slots;
    keys as `Play_KeyIcon_00` under the score [departure]), course card `Play_CourseIn_00` on
    black (name, maker, condition sentence + icon, lives) at the start and after each death,
    pause `Cmn_PauseMenu_00` (close = Continue, Start Over / Retry after a checkpoint, Exit
    Course; course info, condition, best time; game cursor), clear `Play_CourseClear_00` +
    `Play_ClearInfo_00` (banner, clear time, Start Over / Quit), give up `Play_CourseRetire_00`
    (Exit / Keep playing), "TIME'S UP!" `Play_TimeUp_00`, `Play_GameOver_00`; the drawn HUD and
    SwiftUI menus stay as the fallback (`MMK_UI_SMM2=off`, no import); captures
    `MMK_UI_SCREEN=course-*`
  - ✅ import v2: ReferenceMsg tables (clear-condition sentences, part names), the five styles'
    clear-condition icons (`ui/textures/MissionIcon/`); Switch bitmap-font sheets flipped
    (stored bottom row first) and read as luminance; player: caller picture textures, material
    colours, text width; FFI `mmk_ui_table_message`, `mmk_ui_set_picture`,
    `mmk_ui_set_material_color`, `mmk_ui_text_width`, `mmk_engine_play_status`
  - ⬜ capture-based effects (the clear / give-up wipes, pause blur: PF_* panes), the 3D player
    figure on the course card, P-meter / coin pop / 1-Up animations, checkpoint-flag and
    condition-met pop-ups in the course, likes / comments (network)
  - ⬜ FLCT per-character text, glyph borders; title and in-course screens in `mmk-play`
- 🔄 All five styles (SMB1, SMB3, SMW, NSMBU, 3D World) and all themes (day/night) — all five
  playable — sample 3D World 100/100
- ✅ Warp Box (97-warp-box.md): pairs 2k ↔ 2k+1 in the same area, entered by touch (no button),
  player hidden at once, held items dropped, key boxes spend one key and stay open, a used pair
  vanishes (single player); blocked-exit cancel ⬜
- ✅ Koopa Troopa Car (72): drop in from above; start spin, then it runs by itself (≥ 2.0, 3.0
  with the travel direction held [inf]), steering only by a 30-frame turn; jump hops it [inf];
  Spin gets out; runs enemies over; hits damage the car (third breaks it) [inf]; "goal in a
  Koopa Troopa Car" condition. Enemy drivers ⬜ (none in the sample)
- ✅ ! Block (119): 2×2 solid with a drawn path; a hit from below bounces it (7-frame ramp),
  knocks out enemies on it and grows a chain of 2×2 blocks along the path, restored after 600 frames
  - ✅ Pass 3: the chain as decoded — one segment per path point, a head hit slides one out
    (3 px/f), a ground-pound-type hit sends runs to each turn; snake / track-block path codes are
    steps into each node, end codes included · world wiring of `hit_all` (ground pound) ⬜
- ✅ Tree (106): the trunk climbs like a vine up into the canopy, the canopy is a jump-through
  platform, reaching it knocks out the canopy item (vy 4.5) [inf: trigger]
- ✅ Mushroom Trampoline (113): top-only platform N tiles wide; landing launches at 6.0 (4.43 with
  jump held), a jump within 6 frames re-launches at 4.43 − 0.1/frame; enemies bounce 5.5 (6.5
  winged); unflagged ones sway as lifts [inf]
- ✅ Crate (112): solid to stand on, picked up with Run beside it, thrown (3.5 px/f [inf]) it
  breaks on a wall or an enemy and releases its contents (vy 3.5)
- ✅ Track Block (85): 2×2 solid on its own path (2 tiles per node [inf]), sets off when ridden,
  0.5 px/f while ON, holds while OFF, 60-frame stop at the ends, then back (normal) or blink
  180 f, vanish and respawn at the start (alternate)
- ✅ 3D World enemies (actors/w3.rs, world/w3.rs): Ant Trooper (wall/ceiling crawl, horned),
  Stingby (patrol, alert, charge), Skipsqueak (+ Spiny; jumps with the player), Hop-Chops,
  Porcupuffer (4 HP, homing, surface patrol), Bully (push-back, ledge teeter), Charvaargh (lava leap with
  8 segments), Piranha Creeper (path creep, sleeping form) — guesses burnt down against the
  binary (2026-10-18): animation-timed states, Charvaargh's real arc, creeper pieces and node table,
  Bully push table / sight box / ledge teeter; second pass: Charvaargh's vertical and chained
  leaps, turns and spacing, creeper PreStop and trample rules, winged Bully and Ant Trooper
  flight, the horned Ant Trooper's top; what is left is listed in DECISIONS.md
  - ⬜ Hook `World::w3_charvaarghs` into `World::step` (several Charvaarghs' turns and spacing);
    Piranha Creeper stalk as solid terrain, block bumps trampling the head
- ✅ Fire Bars, Burners, airship Cannons and Bill Blasters on tracks: a carrier actor rides the
  track as the moving solid and the fixed object follows it — sample 550/550 by the coverage check
- ⚠️ The 550-course sample dates from June 2019 (game versions 1–3, before the 2.0/3.0 updates),
  so "550/550" says nothing about the update parts: 74, 78–81, 111, 120–132 and the Master Sword
  are absent from it and not yet played (🔄 enemies/blocks in progress; power-ups after Phase 11)
- ✅ Story Mode parts: the stone (75, heavy carry: thrown ≤ 0.8 px/f, never breaks), the escort
  Toad (73, follows 32 px back along the player's path; losing him fails the run [inf]), Toadette
  at the goal (115, scenery); conditions: shake N trees, goal holding the stone, goal on a Lakitu's
  Cloud, no swinging claw, no water — built-in courses 159 → 177/180
- ✅ Empty shells (the 0x4 forms of Buzzy Beetle, Spiny, Dry Bones): never wake; Buzzy / Spiny
  shells met with the head become helmets (take one hit; the Spiny one knocks out what it rises
  into), the Dry Bones Shell is ridden like a shoe [inf: mechanics not decoded]; Buzzy shells now
  drawn with their own art; helmet / Dry Bones Shell goal conditions
- ✅ Alternate forms (flag 0x4) that were silently ignored: Goombrat (turns at ledges), sideways
  Spring (flings 4.5 px/f [inf]), Bull's-Eye Blaster / Bull's-Eye Banzai (homing 1° / 0.25° per
  frame for 528 f), red Cannon (2.4 px/f balls), fast Spike Top (0.75), lit Bob-omb (walks lit),
  Boo circle (8 Boos, 52 px), Blooper Nanny (4 babies on her path), Sledge Bro (hover, hip-drop,
  landing quake stuns), Superball Flower (SMB1: ball reflects at 3 px/f, 270 f, collects coins),
  red POW (breaks on-screen bricks [ext]); the coverage check now reports any unhandled 0x4 form
- ✅ Big Mushroom / Big Mario (SMB1 part 44): the item walks like a mushroom; Big Mario has the
  class-3 sensors (feet ±9.9, wall 11.9) and body box (±12), drawn 32×32, knocks out what he
  touches and breaks bricks he walks into [inf]; "goal as Big Mario" condition
- ✅ Winged Hidden Blocks: invisible flying blocks revealed by a bump — built-ins 180/180
- ✅ Super Hammer / Builder Mario (116): item walks like a mushroom [inf]; Run swings the hammer
  (hits enemies and breaks blocks in front), air swing falls ≤ 1.5 and |vx| ≤ 1.125; "goal as
  Builder" condition; drawn with the builder outfit + hammer meshes. Crate building ⬜
- ✅ Super Bell / Cat Mario (3D World): the bell walks like a mushroom; wall climb (1.0, 1.5 with
  run, ≤ 120 f / 120 px, top-out vy 4.8, then WallClimbFall −0.085 to −2.0), dive (Down in the
  air: 4.0 forward, −3.0), slide scratch (2.5 for 12 f) and air scratch (vy 2.4, gravity −0.1, once
  per air time); "goal as Cat" condition; drawn with the cat suit's meshes and Climb animations
- ✅ 3D World two-stage run: 2.25 until the dash stage (75 f, 60 as Cat), then 3.0 (star 3.6),
  ×1.05 jump while dashing
- 🔄 Every part (0–132, incl. 2.0/3.0 items from the update dump): the 2.0/3.0 enemies, blocks
  and gizmos ✅ (below; from the v3.0.3 binary, editor-built test courses in
  `crates/editor/tests/v3_parts.rs`, `MMK_WRITE_COURSES=<dir>` writes them) · the update
  power-ups ✅ (below; `crates/editor/tests/v3_powerups.rs`) · Koopalings in Clown Cars ⬜
  - ✅ Spike (74): SMB1/SMB3 Gabon3rd walks 0.5 and throws in bursts of 2 when the player is
    32–288 px ahead (ball flies level at 1.5); SMW/NSMBU/3D World GabonWU stands, faces the player
    (3D World 50-frame Turn), waits 0–59 then 120 (SMW 160) + 0–29 frames, lifts and throws on the
    style's animation frames; balls roll 1.2 (slopes speed them up, walls break them, landings
    bounce ×0.6); snow theme: snowballs that are kicked (1.7–5.2) or carried; the 0x4 form is the
    ball itself
  - ✅ Pokey (78): 1–8 segments (default 4), 0.25 px/f, turns at walls and ledges, ±1.5 px sway;
    stomps only on the head (whole stack), fire/shells/tails/slides knock out one segment and the
    ones above hop down; Snow Pokey (ground pound defeats it, Run picks up a snowball); winged hops
  - ✅ Mechakoopa (111; kind = (flags >> 18) & 3): walks 0.5, re-aims every 60–80 frames; stomps
    and ground pounds knock it down for 480 frames (slide/bounce rules, shake below 61, 18-frame
    revival), kicked or carried and thrown while down; fire-proof; Blasta's missile (three-phase
    homing, explodes at frame 300 breaking blocks like a Bob-omb) and Zappa's laser (69-frame
    telegraph, grows 10 px/f to 320 for 40 frames); winged patrol; night-Ground hops
  - ✅ P Block (79, 3D World): solid / outline swapped while a P Switch runs (both kinds); P Switch
    time 540 frames from the binary
  - ✅ Dash Block (80, 3D World): standing on it sets a 120-frame dash at 4.0 px/f along the
    player's facing (jumps keep it; a wall or a turn ends it)
  - ✅ ON/OFF Trampoline (132, 3D World): bounces like the Mushroom Trampoline only while its
    colour's ON/OFF state is on, else a plain platform
  - ✅ Koopalings (120–126): shared action (3 HP, 5 fireballs per HP (big 10), hop between
    volleys of wand magic (1/2/3 shots at 3/2/1 HP, aimed ±10° within 60°), shell after a hit:
    65-frame wait then 1–3 shell hops (NSMBU: 60-frame flinch, 300-frame shell slide), Key drop);
    Larry (higher 1-HP jumps), Wendy (bouncing rings, 1/2/3 alive), Iggy (walks, fast magic),
    Lemmy (rides his ball, 1/3/5 bouncing balls), Morton (ground pound: 80-frame stun, two
    crawling flames), Roy (dives into the ground and comes up elsewhere), Ludwig (hovering shell,
    fans of 3/5 slow shots) · Clown Car rides, Roy's ceiling exits, Morton's winged feints, wing
    flight ⬜
  - ✅ Update power-ups (v3.0.3; `sim/src/player/powerups.rs`, `actors/powerups.rs`,
    `world/powerups.rs`): part 81 gives the style's form (SMB1 SMB2 Mushroom, SMB3 Frog Suit,
    SMW Power Balloon, NSMBU Super Acorn, 3D World Boomerang Flower); a ? block swaps it for a
    Super Mushroom for small Mario, but not the Master Sword or the wearables; every new form drops
    to Super when hit; Link and the balloon have the small body
  - ✅ SMB2 Mushroom: stands on carryable enemies [ext], Down + Run lifts them (3 / 23 frames, carry
    height to 25 over 12 frames), letting go of Run throws (2.2 px/f, up to 4.0, gravity −0.1875)
    and thrown enemies knock out what they hit; 60-frame crouch charge → vy 4.1 with the held-jump
    gravity (≈ 98 px); snowballs pulled from snow ground · cloud pulling ⬜
  - ✅ Master Sword (part 20 with flag 0x4 in SMB1) / Link: 8-frame slash (23 × 15 box, the tip
    breaks blocks on frames 3–7), Down + Run dash (60-frame wind-up, then 3.0 px/f while Run is
    held, wall recoil), shield while Down on the ground (blocks projectiles, turns enemies), bow
    on Spin (24-frame draw, arrows 5.0 / 0.9 straight, 3.5 / 4.8 up, 3.5 / −2.0 down, gravity
    −0.12, drag 0.038, bounce off terrain), down-thrust (12 + 5-frame hang, drop at −6.0, Up
    cancels, bounces off enemies), Up + Run pulls a LinkBomb (Bob-omb blast); EnemyDB sword /
    sword_under / shield / arrow reactions
  - ✅ Frog Suit: 20-frame hops / 10-frame waits on land (3× accel, ice has no effect, no slope
    slide), every jump 3.928; free 8-way swimming 1.0 (Run 2.0) with no gravity, kept 17 px under
    the surface, jump out from the surface
  - ✅ Power Balloon: floats 0.5 px/f along the stick with no gravity or timer, Run jets 2.0 (1.5
    in water; sky-night 3.5) along the tilt, which turns 6° / 5° a frame; walls, floors and
    ceilings bounce it (jet lockout 40); stomps only drift it
  - ✅ Super Acorn: Spin in the air boosts once per air time (vy 2.0, turning to 67.5° while the
    speed grows to 4.2 × base for 15 / 20 frames, then a −0.12 coast), glides with jump held (fall
    cap −2.0 … −0.65 by speed), flutters at −1.5 once the boost is used, clings to walls for 90
    frames; ground spin jump 4.15
  - ✅ Boomerang Flower: one boomerang at a time, 5.0 px/f out 160 px, 15-frame stops, back past
    the thrower 80 / 60 / 40 / 20 px, caught after 15 frames; wall rebound, items hop
    ("BoomerangReaction"), EnemyDB boomerang reactions and bound_boomerang
  - ✅ Wearables (3D World, 127–131): put on by touch (small Mario too), take one hit; turn off
    fireballs, boomerangs and the cat's moves; Cannon Box (30-frame cooldown, 3.0 / charged 5.5
    shots for 30 frames, 32 × 32 blast), Propeller Box (3 flights per air time, 15-frame rise to
    2.5), Goomba Mask (hidden from Goombas / Boom Boom), Bullet Bill Mask (300 frames of 3.0 px/f
    dash per air time while jump is held, breaks on walls), Red POW Box (3 head bumps: POW quake
    and an 80-px block-breaking ring) · the boxes' worn models drawn above the head
  - ✅ Binary pass 3: in-view flag (box touches a camera view, edges in), Spike balls leave the
    model's GeneratorPosition joint, Mechakoopa weapons start at its feet (no "shot" joint),
    Zappa's Shot ends with the beam's (38 f), winged Pokey hop counter resets, Roy one exit try
    per frame, quake stun skips invincible players; missile blasts, carried and thrown parts hit
    like shells; a Bully pushes a shell back; a plain stomp kicks a snowball
- ⬜ Yoshi variants, shoes, clown cars, etc. (rest of the "every part" list)
- 🔄 Course Maker (editor): core crate `editor` — each step tested by saving the course, re-opening
  it byte-identically and playing it in the sim (`unsupported_features` empty):
  - ✅ parts: place / erase / resize (semisolid, mushroom platform, bridge, pipe, belt, lift, slopes,
    vine, Bill Blaster, seesaw, trampolines, snake block, tree) with sizes, forms and composition from
    the game's editor part table; big parts stored doubled; child items; ground auto-tiled
  - ✅ both areas: main / sub switching, sub-area theme / night / vertical (48 wide, 28–168 tall in 7s),
    area sizes (24–240), goal follows the right edge
  - ✅ links: pipes (length / direction / colour, main↔sub warp ids, contents), door pairs (normal,
    P, key), warp box pairs, link ids shared with tracks — encoded like the sample courses
  - ✅ tracks (straight + 45°, corpus end codes, open / closed ends, parts on tracks: 0x400 + piece
    link id) · snake block / ! block / track block / Piranha Creeper paths · ⬜ laying bends and
    junctions, clear pipes, Lakitu-cloud / clown-car / claw riders (kept when editing a file)
  - ✅ selection: rectangle copy / cut / paste / move / delete (parts, ground, tracks; all or nothing)
  - ✅ settings: clear condition (category / CRC32 / magnitude), autoscroll incl. custom nodes and
    speed, liquid mode / speed / heights, timer 10–500
  - ✅ placement rules: the game's per-area part counters from v1.0.0 (100 enemies & gizmos, 100
    power-ups, 2000 plain blocks & coins, 200 terrain parts, 1 checkpoint, 5 pink / big coins, 4
    door and 4 warp-box pairs, 3 Bowsers, 10 path blocks, 5 snakes …; each reached exactly in the
    sample), the validators' size ranges, style-only parts, header ranges (timer, start / goal
    heights, area sizes, custom autoscroll main-area only); overlap layers, start / goal zones and
    containers from the sample · v3 parts' limits (120–132) and per-part flag masks ⬜
  - ✅ shell maker UI (`mmk-play --edit <file>`): engine-drawn area, ghost preview, 40-part palette,
    keys for everything above (README) · SMM2 maker layouts ⬜
- 🔄 Story Mode (Peach's castle): core ✅, screens ⬜
  - ✅ BFEVFL reader (`mmk_formats::bfevfl`, all 24 Story + 51 lesson flows parse) and a flow
    interpreter (`mmk_story::flow`: host-implemented actors, waits, sub-flow arguments, forks)
  - ✅ the game's own flows run the Story (`mmk_story::Story`): jobs activated by the Chief, the
    givers and the Taskmaster, builds enabled by the flows, NPC chains and moves (move flags
    settle when the hub is rebuilt), the three crises blocking the Chief, the kidnapping, the
    ending and staff roll, Builder Mario, post-game jobs 117–119 and the ten 1000-coin statue
    stages, the all-clear Coursebot gear; the rules behind the flows' queries / actions: payment
    on a job's first clear, collected coins on every clear, a failure pays nothing and passes no
    day, builds paid in full when ordered and counted down per cleared job in parallel, pop-up
    queue, coins / days 0–9999, hub gates, hub coins and daily flags
  - ✅ a job's course: 5 lives, TOO BAD → Try Again / Call Luigi / Give Up, Luigi's clear stamps
    the job until Mario clears it, "struggling" play type after two attempts (`JobPlay`)
  - ✅ `StoryState`: quest.dat's meaning as JSON (jobs, builds, days, coins, ordered job, flags,
    talk words, gear, Builder / Superball unlocks)
  - ✅ import stage `modes` (importer v6; also `mmk import-modes <romfs> <cache>`): flows, jobs /
    builds / flags with English fields, the hub map, the clear-condition copy
  - ✅ tests: synthetic flows for every rule; the whole Story from the opening to the post-game
    through the owner's flows (467 courses) when a cache is present
  - ⬜ screens: hub (2D list of `hub_things()` or the 3D hub), Taskmaster list / job window, Chief
    build menu, reward / progress pop-ups, TOO BAD window, ending, staff roll; castle progress %
    (map-part weights not found yet)
- 🔄 Coursebot: core ✅ (`mmk_modes::coursebot`: 120 + 60 slots behind the game's slot table, drag =
  swap of My Courses entries, files never renamed; metadata with clear check, upload code, Super
  World, Downloaded server / local stats; course-number space; scene reasons → list modes; upload
  gate; JSON; `CoursebotStore` over a pluggable blob store; starts empty) · screens ⬜
- 🔄 Lessons (Yamamura's Dojo): core ✅ (`mmk_modes::dojo`: catalogue from the imported
  `lessons/index.json`, 3 levels × 15 Maker Lessons, the 4 power-up lessons, Maker Basics, lesson
  starts with descriptors, completion bits, Maker Basics → 10 title courses, Nina's flag;
  `mmk_story::lesson` runs a lesson flow for the screen — all 50 run to `EndLesson` in the test)
  · Dojo menus, lesson screen (editor puppeteering), `SPRH` demo replays ⬜
- 🔄 Endless Challenge: its menus and map present the 100 Mario Challenge (above); SMM2's own
  rules ✅ (`mmk_modes::endless`: 5 lives [ext], pending 1-Ups max 3 per course paid at the goal,
  misses / Start Over cost a life, free unlimited skip rolled back to the course-start snapshot (a
  skip during a miss demo costs the life), 50 points per second left, quit restores the snapshot,
  skip hint on the 4th attempt, 48-entry history and map position, clear-rate pick rule, offline
  `Session`) · screens on the SMM2 rules ⬜
- ⬜ Ninji speedruns / event courses (offline with local data), records and medals
- ✅ Pass 3 (world.rs, from the binary): blast box, Clown Car exit, cape/hammer boxes, Burner
  collider, helmet hits, Bill Blaster generator, spawn profiles, escort Toad comeback, goal pole

### Phase 10 — Online with an open-source server ⬜ (owner request 2026-10-18)
Not Nintendo's network: the engine talks to an open-source server the owner can run or join.
- 🔄 Server: our own open-source course server `mmk-server` ✅ (directory store, REST API: upload
  with validation, search/sort, download, play/clear/like stats → difficulty, 100 Mario pools by
  fail-rate band; `mmk server`) and client crate `mmk-online` ✅ (`mmk upload/search/download`,
  `RemoteSource` for 100 Mario) · accounts ✅ (name + secret token, SHA-256 stored; maker IDs;
  like/boo, clears with world records and first clears, comments, maker profiles, delete own
  course; client + `mmk register/like/comment/maker`, `mmk upload --token`) · leaderboards ✅
  (maker points [inf weights], courses cleared, first clears, world records, Endless Challenge per
  difficulty; `mmk rankings`) · verified records ✅ (`mmk_modes::replay`: a clear sent as its
  per-frame inputs, re-simulated by the server, which takes the time from the replay; record runs
  served for ghosts; `--verified-records`; `mmk-play --code … --token` submits on clear) · ghosts ✅
  in the portable shell (`mmk-play --code … --ghost` races the record's run, `--ghost-file`,
  `--record-replay`; drawn half transparent, 2D styles; the 3D styles' player is a mesh ⬜) ·
  ghosts in the Swift app ⬜ · server hardening ✅ (request thread pool, CORS preflight for browsers,
  per-address limits on writes and new accounts → 429, `--trust-proxy`, `/api/health`) ·
  SMMDB import ⬜ · ⏳ owner: keep this or connect to another open-source server
- ✅ Endless runs held by the server: `/api/endless` status, `/<d>/start|next|result|suspend|
  finish|history`; batches of 10 by clear-rate band (≥ 20 attempts), no repeats in a run, never
  the player's own or banned courses, widening reported; best per difficulty feeds the
  `endless_<d>` rankings; Endless plays count in course statistics; `mmk-online` client calls
- 🔄 Course World client: browse (SMM2 course list over the server and the library) and play ✅,
  play / clear reports ✅; in the Rust front end (`mmk-front`, 2026-10-18) ✅: account (player
  card → `Nwk_PlayerInfo_00`: create with a typed name, sign in with a token, sign out; token in
  the host's store), like / boo on a server course's clear screen and pause menu (the game's
  judge buttons),
  comments (`Nwk_CommentViewer_00`) and posting (`Nwk_CommentPosting_00`, typed), course page
  (`Nwk_CourseListWindow_00` + `Nwk_CourseDetail_00`: likes, plays, first clear, world record),
  maker profiles (their uploaded courses), leaderboards (`Nwk_CourseListMaker_00`: maker points,
  clears, first clears, records, Endless per difficulty), clears sent as replays the server
  verifies (fallback: the time), the record's ghost raced from the course page, Endless runs
  reported; `mmk-play` host on threads (`--server` / `$MMK_SERVER`), tested against a local
  `mmk server` with captures `account`, `courseworld-leaderboards`, `courseworld-course-page`,
  `-comments`, `-post`, `-maker`, `-play`, `-pause`, `-clear` · comments from the pause / clear
  screens ⬜ ·
  upload from the editor ⬜ (no Course Maker in the front end yet) · the Swift app ✅ (the same
  screens through `mmk_front_*`; its host makes the calls on threads, records server-course runs
  and sends them as replays — tested against a live server in `mmk-ffi`: a verified record, a
  like, a comment; the account screen signed in from UserDefaults; macOS types from the keyboard,
  iOS through a text prompt) · ghosts in the Swift app ⬜ (the engine snapshot draws one world) ·
  the server times a replay to the end of the clear sequence, the clear screen to the goal touch
  (16.35 s vs 9.18 s in one run) ⬜
- ✅ Local multiplayer (co-op, up to 4: Mario, Luigi, Toad, Toadette as the models' _2/_3/_4
  variants): extra players run the same rules, rejoin beside player 1 [inf: bubble], any player
  clears; `mmk-play --players N` (one pad each), FFI `mmk_engine_set_players / step_players`;
  `World` is `Clone` and replays deterministically (rollback groundwork) · versus, bubbles ⬜
- 🔄 Real-time online play: rollback netcode crate `mmk-netplay` ✅ (predict, restore snapshot,
  re-simulate; redundant input packets over UDP; test: two peers with 4-frame delay and 1/3 loss end
  identical to a full replay) and `mmk-play --net-bind/--net-peer/--net-player` · up to 4 players
  ✅ · relay rooms on the course server (UDP, same port) with hole punching ✅ (direct once a peer
  answers a probe, relayed until then; `mmk-play --net-relay host:port --net-room NAME`; test: 4
  peers through a live relay end identical to a full replay) · browsers ✅ join the same rooms over a
  WebSocket (`mmk server` HTTP port + 1; page "Play online…" or `?relay=&room=&player=&players=`;
  checked with two headless browsers, one joining 3 s late: identical positions at frames 600 and
  1200) · start sync (`MAX_LEAD` wait) and acked resends ✅ · desync checks ✅ (state hashes at
  confirmed frames every second, compared across peers, logged; `mmk simulate --checksum` run in
  two processes over the 550 courses: identical) · WebRTC upgrade, versus ⬜
- ✅ Accounts without Nintendo services; moderation for self-hosters (`--admin-token`: ban/unban,
  delete any course)

### Phase 11 — SMM1 extras, each behind a setting 🔄 (owner request 2026-10-18)
From the owner's SMM1 (Wii U) dump, as engine options layered on SMM2 play (`mmk_sim::Options`,
all off = SMM2; FFI `mmk_engine_set_option`, `mmk-play` / `mmk simulate` flags). Rules from
Block.rpx v208 (private spec research/specs/smm1-extras.md).
- ✅ Mid-air shell kicks / shell jumps (`smm1_shell_kicks`): SMM1's flat 10 px falling stomp
  window, still shells kicked without a bounce, the kicker's 16 / 8 / 4 no-hit windows and the
  5-frame contact window, the 4.0 catch-up re-kick, and the mid-air wall assist (hop 3.51855, the
  shell parks a tile short of the wall) — the "shell jump"
- ✅ SMM1 draw order (`smm1_layering`): SMM1 draws the player in front of pipes in play (the
  "pipes over the player" memory is the editor, and pipe travel); the option applies SMM1's
  depth ladder: passing through a pipe the player is in front of blocks but behind the pipe,
  coins behind blocks, tracks over pipes, the vine / pole / death depths
- ✅ Lanky Mario (`smb1_mushroom=weird`, SMB1 courses' Super Mushrooms become Weird Mushrooms):
  Super's body, jump launch +0.5 (≈ 6.3-tile standing jump), float cue, SMM1 pickup/damage rules,
  the narrower item box; art imported (`M1_Player_SlimMarioMdl`, `M1_Item_kinokofunny`)
- ✅ Costumes (`smb1_mushroom=mystery`, `costume` = id or random): small Mario's body and physics,
  crouch, air turn, Up taunt, no helmets, Sonic's spin; costume ids as SMM1's; random pick from the
  costumes with art; art (115 costumes, 24 × 24 frames) and sounds (jump/jump2, power-up, taunt,
  landing, skid, death and clear jingles) imported by `mmk import-smm1` into `<cache>/sprites`
- ⬜ Swift Settings toggles and the costume picker (FFI ready; app being rewritten separately)
- ⬜ Options in replays / online play (replays and rollback sessions don't carry options yet: keep
  them off for records)
- ✅ 100 Mario costume rewards (smm1-100mario-code.md §7): a finished run wins a costume not yet
  owned, uniformly among the categories the level draws (Easy 1, Normal ≤2, Expert ≤3, Super
  Expert 1/2/3/6; never 0/4/5/7), kept as `costumes.owned`, announced after the ending with
  SMM1's two "pool empty" notes (both hosts) · the random costume drawing only from owned ones (SMM1) as a setting ⬜
- ✅ Lanky Mario's jump, float-cue and growth sounds (`M1_HEN_JUMP`, `_FUWA`, `M1_HEN_LARGE` from SMM1's
  `Pack/Sound.pack`, SMM1 import v3) · ⬜ pickup freeze length
  for costumes [?]; Inkling squid frames while swimming; the random Weird Mushroom easter egg
  (1 in 20, pity at 100; its gates are not understood)
- ⬜ SMM2 mode fixes found on the way (fidelity log): still-shell stomps shouldn't bounce, SMM2's
  kicker windows and wall assist
- ⬜ Other SMM1-only behaviours (surveyed, research/specs/smm1-extras-misc.md §B): per-enemy pipe
  spawn intervals (Goomba 140, others 180, no pipe colours); SMM1 spawn caps; Clown Car weight
  classes; attack reactions (shells vs Lava Bubbles, Blooper stomps, Fish Bones in lava, star vs
  Munchers); helmet reactions; behaviour presets (Green Koopa, Spike Top, pipe Blooper, Dry
  Bones, Magikoopa); stacking heights; SMM1 sound effects (14 built-in + recordings); Big Mushroom
  in other styles [?]

### Recomp — native SMM2 v3.0.3 by static recompilation 🔄 (owner request 2026-10-18)
Separate from SMCraft, in `recomp/`; approach in DECISIONS.md (base: LAS Recomp, MIT; Wonder
framework ideas; Ryujinx as the reference). Nothing counts as done without the evidence below.
- ✅ Approach chosen and recorded; proprietary-content guard extended for recomp output
- ✅ Toolchain: LAS Recomp forked into `recomp/` (MIT notice, upstream commits in `recomp/UPSTREAM.md`), retargeted
  to SMM2 v3.0.3 (`main` + `sdk`), shared runtime + `games/smm2/`; one command from the player's dump:
  `make -C recomp smm2` (setup, recompile, build), `make -C recomp run-smm2`
- ✅ R1 (macOS): boots to the title screen and renders it (2026-10-19). What it took: `recomp/docs/smm2-bringup.md`
  (per-core scheduling, nifm/friend/mii/fonts/system data, NVN conditional rendering, 3D render targets, blocking
  first-use compiles, narrow vertex strides, in-order uniform updates, translator TLD4/TMML/LDC.64 fix)
- 🔄 Title screen frames vs Ryujinx, five courses in four styles (3D World, SMB1 x2, SMB3, NSMBU), same course
  and nearest frame, both windows grabbed through the window server: 2.50-4.80 % of pixels differ by more than 24,
  0.66-2.98 % by more than 48, 0.13-1.15 % by more than 96 (table in `recomp/docs/smm2-bringup.md`). Fixed in
  this round: skinned meshes (Mario, Koopa Car: signed LDC offsets), a culled stand-in mesh, geometry shaders,
  2D tile sheets (rendered textures keep their memory). Left: texel-edge and text differences, animation phase
  (not exactly alignable: Ryujinx grabs are wall-clock timed). Ryujinx against itself: 0.3-1.8 %.
  Evidence: `recomp/private/smm2/evidence/r1/final_*` (`_side_by_side.png`, `_diff.png`, `_recomp.png`; private,
  not in git)
- ⬜ R2: play a course by input injection; input-replay comparison against Ryujinx
- 🔄 Mods: 100 Mario Challenge on SMM2's Endless Challenge (100 lives, SMM1 run rules, courses from
  our server or the local library); Mystery Mushroom costumes (model, sounds, physics swaps)
  - ✅ Server side: Endless runs take a ruleset at `/start` (`smm2`, or `hundred`: 100 lives capped
    at 100, done after 8 / 16 / 16 / 6 clears with `run_complete`, SMM1's fail-rate bands, skips
    free after a death); completed runs on `hundred_<d>` leaderboards apart from SMM2's Endless
    boards; rankings carry the caller's own row (`me`). SMCraft's 100 Mario reports completions
    there too (tests: modes, server, front)
  - ⬜ Recomp hooks (the game's Endless NEX calls answered by our server, lives text and cap,
    skip rule); ⬜ costumes

| Platform | Builds | Boots to title | Plays courses | Frames match Ryujinx | Input replays in sync |
|---|---|---|---|---|---|
| macOS (Apple Silicon, Metal) | ✅ | ✅ title rendered, 60 fps | ⬜ | 🔄 title, 5 courses / 4 styles: 2.5-4.8 % px >24, 0.13-1.15 % >96 | ⬜ |
| Windows (x64, Vulkan) | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |
| Linux (x64, Vulkan) | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |
| iOS / iPadOS (arm64, Metal, AOT) | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |
| Web (wasm + WebGPU) | ⬜ feasibility decided after the native ports | | | | |

---

## Phase 1 detail

### Done
- Repo initialised. Public home: https://github.com/smcraft-git/smcraft (published as snapshots;
  2026-10-18). `.gitignore` excludes dumps, keys, extracted data and `research/`.
- Proprietary-content guard (`scripts/check_no_proprietary.py`) + tests + CI workflow + pre-commit hook.
- Toolchain installed and pinned outside the repo — see [docs/TOOLCHAIN.md](docs/TOOLCHAIN.md).
- `tools/inventory.py`: identifies extracted files by magic (through Yaz0/zstd), looks inside SARCs.
- **SMM1** (WUX disc image) decrypted with console common key + disc key:
  - `~/smm-work/extracted/smm1-disc/` — base game v16 (`000500001018DC00`)
  - `~/smm-work/extracted/smm1-update-v208/` — update v208 bundled on the disc
  - Detailed inventories: `research/inventory/` (private)

### SMM1 layout summary (base v16 + update v208)
| Area | Where | Format |
|---|---|---|
| Main executable | `code/Block.rpx` (v208: 9.0 MiB) | Wii U RPX (PPC, big-endian) |
| Course data | `content/Course/*.szs` (152: sample, tutorial, title, new) | Yaz0 → SARC → `.cdt` (SMM1 course) |
| Course thumbnails | `content/CourseThumbnail/*.szs` | Yaz0 → SARC → `.tnl` |
| Tilesets / scenery | `content/Model/M1_Field_*`, `M1_DV_*` etc. per style | Yaz0 → SARC → BFRES |
| Sprites / characters | `content/Model/*_Player_Chara_*`, `Pack/*.pack` | BFRES, SARC packs of Yaz0 SARCs |
| Per-style environments | `content/Pack/{M1,M3,MW,WU}_FieldEnv_{Normal,Dark,Water,Yogan}.pack` | SARC |
| Physics / object params | `Pack/*.pack` → `.params`, `.byaml` | text params, BYML (to be confirmed in Phase 3) |
| Text | `content/Message/*_USen.szs` | Yaz0 → SARC → MSBT |
| Layout / UI | `content/Layout/*.szs` | Yaz0 → SARC → SARC (BFLYT/BFLIM) |
| Music | `content/Sound/**/*.bfstm` (147) | BFSTM |
| SFX | `content/Sound/*.bars` (193), `Sound.pack` | BARS → BFWAV |

Styles: `M1` SMB1, `M3` SMB3, `MW` SMW, `WU` NSMBU. Themes: plain, underground, castle, airship,
water, haunted house.

- **SMM2** from `MarioMaker2.NSP` (owner's dump; v1.0.0, title `01009B90006DC000`, SDK 7.4.0, no update):
  - `~/smm-work/extracted/smm2-nsp/exefs/` — `main` (NSO, 17 MiB), `sdk`, `rtld`, `main.npdm`
  - `~/smm-work/extracted/smm2-romfs/` → symlink to `/Volumes/SSD/smm-work/extracted/smm2-romfs` (2.7 GiB; internal disk is full)
  - `SMM2.xci` is NOT used (it is the `[romstorage.com]` file renamed).
- **Course sample**: `tools/fetch_courses.py` pulled 550 courses into `~/smm-work/courses/mm2_level/`
  from the public `TheGreatRambler/mm2_level` dataset (CC BY-NC-SA 4.0): 25 per style x difficulty
  plus 50 extra SMB1 ground courses. All from June–July 2019 uploads, so they only use v1.0.0 content.
  All 550 decompress to 0x5BFC0 bytes and their headers (timer, style, name) match the metadata.

### SMM2 layout summary (v1.0.0)
| Area | Where | Format |
|---|---|---|
| Main executable | `exefs/main` | NSO (AArch64); Ghidra Switch Loader |
| Built-in courses | `Course/*.szs` (120 story `quest`, 50 `lesson`, 10 `title`) | Yaz0 → SARC → `.bcd` (to confirm in Phase 2 whether encrypted) |
| Online/save courses | (from dataset) | 0x5BFC0-byte decrypted course; on-console `.bcd` is encrypted |
| Tilesets / scenery | `Model/{M1,M3,MW,WU,3W}_Field_<theme>`, `_DV_` (background), `_S_DV_` | zstd → SARC → BFRES (+ BFSHA) |
| Theme animation | `Pack/<style>_FieldAnime_{Normal,Dark,Ice,Snow,Water,Yogan}.pack` | SARC |
| Sprites / objects | `Pack/<style>_Model.pack`, `Static*.pack`, `Model/` | SARC of zstd SARCs → BFRES |
| Params / physics candidates | `Pack/*.pack` → 423 BYML; `Bake/*.bkres` | BYML; SARC (to be mapped in Phase 3) |
| Text | `Message/*.szs` | Yaz0 → SARC → MSBT (3038) + MSBP |
| UI | `Layout/*.szs` | Yaz0 → SARC → SARC (BFLYT etc.) |
| Music & SFX | `Sound/Resource/**` (471 BWAV, 2.1 GiB), `Pack/<style>_bars.pack` | BWAV, BARS |
| Sound delay/env per theme | `Sound/Effect/delay_<style><theme>.byml` | BYML |

Styles: `M1` SMB1, `M3` SMB3, `MW` SMW, `WU` NSMBU, `3W` SM3DW. Themes: plain, underground, castle,
airship, water, hauntedhouse, snow, desert, athletic (sky), woods.

- **SMM2 update v3.0.3** (`Mario Maker2 Updatev3.0.3.nsp`, patch title `01009B90006DC800`, NACP display
  version 3.0.3) applied over the v1.0.0 base:
  - `~/smm-work/extracted/smm2-v303-romfs/` (3801 files, 2.8 GiB; adds `MyWorld/`, +22 models, +3 packs,
    +344 sound files, +123 layouts) and `~/smm-work/extracted/smm2-v303-exefs/` (`main` 20 MiB NSO)
  - ELFs for analysis: `/Volumes/SSD/smm-work/extracted/smm2-v100-main.elf`, `smm2-v303-main.elf`

### Open
- Owner's own saved courses: not yet exported from the Switch; the dataset sample covers Phase 2 for now.
