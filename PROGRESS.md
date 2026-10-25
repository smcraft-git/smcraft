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
  Endless map's "New High Score!" panel. Evening of 2026-10-24: blocks, trees and flags release any
  part (dressed), vines grow, Bill Blasters and Lakitus fire / throw any part (coin bursts too),
  enemies drop held Keys, Koopalings ride tracks — coverage 550 / 550 of the sample, 38 / 38 recent
  TGRCode; the game's track tiles; CJK names; the Toad House minigames; SMB3 / SMW wings; boss
  jump sounds by style.
- **GPU-slot queue** (one game instance at a time; the physics agent holds it most of the time):
  ✅ pixel table re-rendered 2026-10-25 (branch `pixels6`, k = 900; 2D means: SMB1 1.6 / night
  1.2 (was 2.4 / 1.8), SMB3 2.7 / 1.7 (5.0 / 2.5), SMW 2.2 / 2.0 (6.1 / 3.0); NSMBU 15.0 / 9.2
  (17.8 / 14.6); 3D World 29.0 (29.5)). Found: the 2D day backdrops scroll by their model's
  texture animation (`<S>_DV_athletic_fts` 4096 frames a picture, SMB3 / SMW day airships
  1024, other airships 512; `mmk bfres --curves` now prints shader-parameter tracks); the course
  scene's animations run 63 frames ahead of the player's first update (`World::scene_frame`:
  backdrop scroll, the coins' spin, the airship's sway — whose zero then falls on the scene's
  first frame — and the 3D meshes); the airship's start ground is the field atlas's cloud set
  (SMB1 / SMB3 / SMW / NSMBU; the goal ground's [?], 3D World's field is meshes); the 2D camera
  snaps to the target's pixels (the sway moves by fractions of a world pixel); NSMBU / 3D World
  backgrounds play their `_fts` texture-matrix tracks (importer 15 / game pack 15, re-imported:
  NSMBU airship 33 → 21, sky 35 → 27, nights −4); the Swift app's own mesh path still draws the
  backgrounds' materials unscrolled (its sky layers do scroll: they come with the scene's light
  block). Left in 2D: SMB3 forest 10.3 (the forest water: the reference's is dark green over
  black, ours blue — translucent over another colour or a tint [?]), castles ~4 (lava), SMB1
  forest 4.4. NSMBU: the sky's grey cloud layers (`kumo_A`–`C`) and big clouds (`nyudo`) still
  sit elsewhere than the reference's (sky 27, airship 21: bones / indirect texture?), idle
  Mario faces the camera in the game (ours 70° turned, ~5 px high), the castle's lava is a 3D
  surface in the game (ours the 2D strip), a global tone offset (ground ~14). 3D World: the
  colours / tone (castle, snow, ground 32–43). ✅ Pass 7 (2026-10-25, branch `pixels7`, k = 900;
  research/specs/render-3d.md "The final pass" / "Environment palette" / "Diffuse light map"):
  3D World 29.0 → 21.2 (ground 34.1 → 20.7, underground 18.0 → 16.9, castle 32.1 → 22.1, airship
  26.3 → 28.5, water 38.2 → 26.2, ghost 22.4 → 17.4, snow 43.2 → 18.9, desert 17.4 → 13.0, sky
  26.0 → 15.1, forest 32.6 → 33.4); 2D styles and NSMBU unchanged. Read from the recomp's draws:
  the 3D styles' final pass (3D World tone-maps 1 − e^(−√2·c) from a float target; both look each
  pixel up in the colour-correction table: saturation about the brightest channel, from each
  env's `.baglccr`; importer / game pack 16) and 3D World's environment palette
  (`Palette/Palette3W.benvpal`: the lights and fogs the game actually uses, not the `.baglenv`
  files'; variant 2 in play, `DV_FogPat` 1 leaves the second fogs out, the forest's code-made
  downward fog); 3D World's lit colour scaled 0.65 [meas] until the diffuse light map is
  modelled (a second `LM_ConstructMem` cube the materials sample at the normal; where the game
  fills it is not found). The Swift app's background materials scroll (`mmk_mesh_shapes_at`).
  Left: the diffuse light-map cube (NSMBU's grey cloud bands are the cube's light on `kumo_*`,
  3D World's overall light level), NSMBU sky 27 / airship 21 / water 21 (the sky dome is 20 %
  white over the bands; light shafts), the 3D liquids (castle lava, forest water: 2D strips in
  ours), 3D World forest's field (bake AO, pebble overlays), castle bloom (lava above 2 in the
  float target blooms: threshold 10·luma − 20), NSMBU idle Mario (faces the camera, ours 70°;
  drawn 5.75 px high in every NSMBU theme, 3D World's on the ground), SMB3 forest water (its
  `top` texture is greyscale: tinted per theme [?]). Captures of the track tiles in play and of NSMBU / 3D World wings (the mesh
  planner maps `M1_Enemy_wingcommon` to `<style>_Enemy_wingcommon`), a look at Water Level mode
  and custom scroll in the recomp (`research/tools/maker-look`: touch taps / drags by script).
- ✅ **Inferred-behaviour inventory** (2026-10-25, branch `infres`, research specs/inf-inventory.md):
  every [inf] / [?] / [meas] in sim/src ranked by course share (823 courses: the sample + tgr2) and
  visibility; 14 resolved from v3.0.3 — the shared fly helper for winged items / coins / map objects /
  blocks (0x71013d0e50), lift start direction and flag-0x2 bob, the winged Spiny's PataTogezo flight,
  the Lakitu coin's flight, the Sun's 300-frame first wait, the cape's 20-frame spin, the Side
  Thwomp's player box (0x7100d7fed0), the Chomp's Bézier return (0x71012b56f0), custom autoscroll
  speeds (0x710179c8f0), the red Yoshi's Egg, the Bob-omb fuse and Key start motion (no change).
  578 / 580 traces exact.
- 🔄 **Inferred-behaviour pass 2** (2026-10-25, branch `infres2`): ✅ the Swinging Claw's rope from
  v3.0.3 (step 0x7101ca03f0, init 0x71013e7780, exe 0x71013e7b10; actors/claw.rs): the actor is
  the rope's end, 6 × 7 px links (L 35, held on L + 4), the pivot 48 px up, gravity −0.1875, a
  turn rate fed by the rope's turn × 0.05 and raised to 0.25° at each crossing (what sways an
  empty claw from rest), × 0.95; the game's binary atan2 table (fmath `table_atan2`); spawn box
  128, cull box 256 × 1024; the catch window / player speed / ±0.05 nudge / hold spacing from
  0x71013e87c0 / 0x71013e8bf0 / 0x71013e7f30 [inf: the held player's hang point]. Both claws of
  W_3010660 exact to frame 235 (where the engine's player, off since 141, dies early). ✅ the
  Twister's player handling from v3.0.3 (player/twister.rs: the player update's twister block
  0x710158a590, contact 0x71013ec330, column 0x71013ed4a0): the released band gravity turned
  upward while rising (not when falling within 16 px of the top), a grounded player lifted off,
  a 30-frame steer toward the Twister's movement (0.075) and centre, a jump in the column +1.0 a
  frame, rise caps 3.0 / 5.0 (Sky night 2.5 / 3.5); the column box widening upward (margin 4)
  [inf: the player's box and shape; the +0x230c bit 0 / +0x494 bit 1 paths]. No twister in the
  traces: 578 / 580 unchanged. ✅ stacks (world/tower.rs, research objects/stacks.md): a column
  spawns only through its manager record's box (bottom part, 8 px × the widest member a side,
  16 px × the summed heights up; 0x71003fbef0 / 0x71003fc820 v1.0.0); the drop-out predicate
  from v3.0.3 0x7101285500 (normal mode only: a stomped Koopa's shell is a new object); a rider
  at a wall keeps the pull, not 0, and the carry (0x7101299dd0); riders have no contact reaction
  (Tower vtable slots 12 / 13 empty) [inf left: a walker meeting a rider]. 🔄 winged enemies
  (actors/wing.rs): ✅ the Bob-omb's own Fly / Turn (v3.0.3 helper 0x71010833a0: 0.5 px/f,
  ±8 px bob eased by 0.03, floor / ceiling flips); ✅ Dry Bones flies the red Paratroopa's Fly
  (Karon Wing = NokonokoRed's); ✅ the enemies whose Wing action is their Basic one (Boo,
  Cheep Cheep, Fish Bone, Wiggler, Magikoopa, Bros, Monty Mole, Blooper, Chain Chomp) act as
  unwinged; ✅ Buzzy / Spike Top (Met Wing: the Spiny's hover helper 0x7100782a80, no wall
  turn); ⬜ their wing-only states (Boo PataCharge, Bros WingWalk), Lava Bubble (Bubble Wing),
  the Para-Beetle's PlayerOn ride. ✅ Bowser's chase jumps (v1.0.0 0x71006ab370: a wall
  on his right jumped by kind — obstacle scan 0x71006abfe0, free run of 3 / 5 tiles 0–2 up the
  middle jump, 3–5 the big one; the gap scan 0x71006ab810 and the gap jump's landing speed
  0x71006ac370, |d| / 2 · 0.1 / vy ≤ 1.8; [inf] the overhead collider queries pass). ✅ the
  Yoshi's Egg hatch: Land the first frame it is on the ground with its 22 × 22 box touching the
  camera view (Wait 0x7100852bb0, vfunc 0x4e8 0x71008cc760), then the helper's 40 frames (v3.0.3
  0x710137acd0 / 0x71013f7990).
- 🔄 **Inferred-behaviour pass 3** (2026-10-25, branch `infres3`, research specs/inf-inventory.md
  "Pass 3"): ✅ the v1.0.0-only findings confirmed in v3.0.3, no change — stacked columns spawn
  through the manager record (0x7100e3fcd0 / 0x7100e40a10, box test 0x7100e41150; the manager is
  actor 0x124, box {0, 8, 8, 8} flags 0x448 at 0x71023ce534), Bowser's chase jumps (0x7101154260,
  obstacle scan 0x7101154ed0, gap scan 0x7101154700, landing speed 0x7101155260), the winged Dry
  Bones = the red Paratroopa's Wing (factory 0x7101135770). ✅ the Boo (actors/boo.rs): a winged
  one's PataCharge at the start and after every Stop (still, aiming, the wing helper 0x7101270eb0
  easing its flap to 2.0 by 0.04, then 20 frames, Follow at 0.8 → 0.4), the aim as the binary
  clamps it (±45° on the facing side, swinging over when the player crosses; starts pointing
  down). ✅ the winged Lava Bubble (Bubble Wing 0x710109f400 / 0x710109f9b0 / bounce 0x710109da60:
  1 px/f diagonal, reflecting off terrain once its drawn rotation has caught up). ✅ the
  Para-Beetle ride (Met Wing PlayerOn 0x71011d73b0 / 0x71011d7480: stood on, dips −1.0, climbs
  +0.04 to 0.5, FlyMove 18 frames after the rider leaves). ✅ the held player's claw hang 22 px
  (23 in other forms; 0x7101587d10 / 0x71015d24c0) and the jump off it (the Crane state's
  ordinary jump start, exit's ±4 / ±2 carry; 0x71015a06a0 / 0x71015a0760). ✅ the Twister in
  water (0x710158a590 +0x230c bit 0: half bands, caps 1.5 / 3.0). ⬜ the Bros' WingWalk (v3.0.3
  enter 0x71010961c0 / exe 0x7101096310, five sub-phases on the Bros action's vfuncs and the
  +0x4e8 helper: not ported), a walker meeting a stacked rider (the action contact slots 12 / 13
  are the same defaults for Basic as for Tower, so pass 2's evidence doesn't decide it), the
  item touch boxes (not in EnemyDB or the item init 0x710134fe30; set by the shared enemy init
  0x7101286ee0) [inf]. 578 / 580 traces exact; coverage 550/550 + 273/273; simulate 0 crashes.
- 🔄 **Inferred-behaviour pass 4** (2026-10-25, branch `infres4`, research specs/inf-inventory.md
  "Pass 4"): ✅ the Bros' WingWalk (actors/bros.rs; v3.0.3 enter 0x71010961c0 / exe 0x7101096310,
  hop 0x7101096ef0, walk turns 0x71010979f0, the Bros1st / BrosU action slots 0x120–0x1a8 and
  helper slots): hops (vy 2.0, 1.0 by the water-speed rule; 0.5 px/f, turned by terrain 12 px
  ahead), one hammer a landing (Bros1st held, on screen, frame 14, 20-frame wait; BrosU after
  JumpSt 12 (3W frame 6) and JumpEd 14, at Throw frame 33 / 26, Throw's end 68 / 60), a jump try
  every third throw (countdown 3). ✅ Bowser's overhead query (0x7101153d40 via 0x7101153860):
  solid terrain (unit class not 0 / 2) over him up to 16 px a free-run row + his BG sensor top
  (box id 0x34 + style: 26 / 31.8 / 34 / 36, 3W 46) stops the chase jumps. ✅ the winged Spike Top
  (Togemet Wing 0x7102911e68: PlayerOn without the rider's contact timer): stood on, dips, hurts its
  rider. 🔄 item touch boxes: the collider table 0x71023cb8b0 by box id +0x6a0 (items 0x5f =
  {0, 7, 7, 7}; overlap 0x7100e28e20): 3D World's box [obs], top 14; the 2D styles' extra pixel and
  the Fire Flower's id (0x12) not found. 🔄 walker contact: push = half the overlap [obs
  0x7100e28e20]; the bump component (+0x3a0) is off in the Tower context, its partner raises the
  turn flag +0x528 bit 19 (0x710128a2d0); its pairing not found. ⬜ the Sledge Bro's WingWalk (its
  own helpers), the red Koopa's ledge reach (the table's ±4 breaks 9 traces; 3.9 stays), Bowser's
  jump-height search (BG type 0x25) [inf]. 578 / 580 traces exact; coverage 550/550 + 273/273;
  simulate 0 crashes.
- 🔄 **Inferred-behaviour pass 5** (2026-10-25, branch `infres5`, research specs/inf-inventory.md
  "Pass 5"): ✅ the Sledge Bro's WingWalk: there is none — MegaBros sends Fall's landing to Walk
  (slots 0x128 / 0x130 0x71011d2710 / 0x71011d2720) and its helpers' Wing gate is off (slot 0x88
  returns 0), so a winged one keeps its action; its wings give the 6.5 jump and a hover bob (vy
  0.4·cos θ from 90° by 8°, Hovering exe 0x71011d2b50); the hover holds 9 frames. ✅ the walker
  turn: the contact callback 0x71011ff860 raises +0x528 bit 27 (Walk 0x7101050c60) on a push ≥ 0.1
  (half the overlap) against the facing, feet within 8 px (3D World: vertical push ≥ 4); a walker
  turns on a tower member level with it; the +0x3a0 component (pass 4's "bump") has the player
  twister receiver's slots, not a pairing. ✅ the red Koopa's ledge probe: the walkers' foot sensor
  ±4 (big 8) with a cell edge closed (replaces 3.9; the open edge breaks 10 traces). 🔄 Bowser's
  height search 0x7101153f00 decoded (actor-owned class-0 BG units of type 0x25 under the jump's
  height block it; the type's maker not found), the NSMBU Turn (`lkp_walk`'s FSKA loop bit is
  clear like all the model's, so the play mode comes from the anim controller; not read), the
  Propeller Mushroom's Move decoded (0x710134ca70 / 0x710134c3c0; not ported). ⬜ a sliding shell
  meeting a boss (#15), the 2D items' extra touch pixel and the Fire Flower's box id (no writer in
  the item code 0x7101330000–0x7101420000). 578 / 580 traces exact; coverage 550/550 + 273/273;
  simulate 0 crashes.
- ✅ **Physics pass 8** (2026-10-25, branch `physics8`, research fidelity-log.md "physics8",
  objects/93-clear-pipe.md "Traced", 91-seesaw.md, 87-gentle-slope.md): **598 / 598 traces exact**
  (the 580 and 18 new whole-course traces), every whole course exact — W_3049200 to its end,
  W_3010660 to its death. ✅ a slope whose low end cap lies under a ground tile has its first step
  full solid (gentle: two cells, steep: one; the floor collider type 0x11 / 0x12 sub 10; v3.0.3
  cell function 0x7100db8df0 replaces a cell whose art is a joint piece) — six arrangements traced
  on test courses (`mmk trace-courses --only M1_slopejoin`, `M1_slopejoins`); ✅ the stomp window
  from the enemy's top before its move; a Koopa stomped in the air falls as its shell that frame;
  a walker stands a frame after a fall only on a slope; ✅ the Seesaw ported from v3.0.3
  (actors/seesaw.rs: physics 0x71013d7d90, integrator 0x71013d9950 — binary angle, the torque
  through the game's atan / sine tables, the chord integrator, the damping before and the return
  to level after the turn, the ends from the table rotation; the rider on the ends' line past the
  turned end, carried by the ends' movement, stepping x · cos of the angle two frames before, the
  terrain sensor's slope classes for its speed; a ground pound's slam ±7.2 / (W + 3) to the stop);
  ✅ Clear Pipes against the game (test courses 3W_cpipe / 3W_cpipeg / 3W_cpipev / 3W_cpiped:
  straight, two bends, joined, blocked, a Goomba, a mushroom, up and down mouths): side entry on
  the face after 10 held frames, Down taken before the move, the entry frame standing, the out
  frame running the next state (standing on an upward mouth; the head at a downward one), held
  buttons no new presses after a ride, enemies' in-pipe speeds 0 — frame-exact; ✅ 3D World Goombas
  spot the player (80 px in front), stand 53 frames, run at 0.7 (the Kuribo3W Find / Run / Lost
  states); ✅ placed Super Mushrooms walk right from their first frame, rest 0.1 px into walls and
  turn a frame late; ✅ a mushroom is touched where it stood before its move, 12 px between centres
  (SMW / NSMBU 13), and a growth freezes the world from the frame the form changes to the frame
  the change ends (test course M1_mushstart, three scripts). ✅ a ground pound needs 16 px of air
  under the feet at the press (WU_seesaw's pound scripts); a crouch walks off a ledge into its
  fall at once. Left [?]: the clear pipe's push entry (0x71015a9d00: 9 frames of pushing didn't
  take the player in), the Seesaw's blocked-rider sum (two riders).
- ✅ **Physics pass 9** (2026-10-25, branch `physics9`, research fidelity-log.md "physics9",
  player-physics.md §5.4b, objects/44-style-powerup.md "Traced", 81-style-powerup-2.md §12,
  91-seesaw.md "Walkers"): the power-up specials against the game — **654 traces, 652 exact**
  (598 → 654: 56 new on new test courses `M3/MW/WU/3W_form`, `3W_formw` / `3W_formt` (the Cat's
  walls), `<style>_form2` (part 81), `M1_sword`, `M1_sball`, `3W_hammer`, `<style>_itemplace`,
  `WU_seesaw2` / `seesaw3`, `MW_balloon`; recorded with research/tools/ryujinx-trace `capform.py`). ✅ SMB3
  Super Leaf flight (TailFlying from v3.0.3: a flap only below vy 0.75, the 18-frame flap timer,
  queued presses, the tail's air mode to the landing, the P-meter frame by frame — hold counted
  twice, walking states only, kept full in Jump / Fall — the 240-frame window, TailAttack's
  landing); the view's top + 96 and sides 8.0 / 8.1 hold the player alone. ✅ SMW cape: the
  take-off frame empties a 29 meter, MantFlying's entry frame, the cape's stage count, Fall's
  first frame without float, Landing → Crouch with Down, Turn's facing. ✅ NSMBU propeller: its
  own air control, the fall spin (−1.0 for 37 frames, 20-frame cooldown), the drill on the
  ground-pound command (≥ 10 frames). ✅ 3D World Cat: the wall climb rebuilt from v3.0.3 (rays
  38 / 28 / 9 px, 1.0 / 1.5, 120 / 60 px, 120 still frames, the 58-frame fall start, the slide,
  the top-out 4.8, wall jump 3.5 / 2.25, air locks), 13.9-px ground probes, no WallSlide, the
  dive's cancel and still landing, the crouch scratch; 3D World's Spin slide / roll (all forms)
  and crawl; a low 3D World area is a low-camera one. ✅ Link's down-thrust on the pound command,
  the Frog Suit's hops, Super Acorn's boost / twirl / spin jump, the Super Hammer's 27 frames, the
  Power Balloon's jet;
  SMB2 Mushroom, Superball and Boomerang exact as they were. ✅ placed Leaves / Feathers /
  Propeller Mushrooms (infres7's Move start) exact, removed 40 px under the view. ✅ walkers on
  Seesaws (weight 2, the riders' torques summed, carried, x · cos). Open: the Cat's rolling attack (0x71015f6f70, its
  trigger not found), where a walker leaves a Seesaw's end [?], a Cat's 15.9 head on the ground
  [?]. Coverage 550 / 550 + 273 / 273; simulate 0 crashes.
- 🔄 **Physics pass 10** (2026-10-25, branch `physics10`, research fidelity-log.md "physics10",
  91-seesaw.md "Leaving the plank"): **675 / 683 traces exact** (was 652 / 654; the 8 others are new enemy traces parting at the game's random draws). ✅ a walker leaving a
  Seesaw's end (v3.0.3 floor test 0x7100e2b3c0 / 0x7100e2ccf0 / 0x7100e32dd0): the plank is one
  floor segment whose open ends stretch by the foot sensor's half width (4) × cos 2u, so a foot
  holds 1.5 px past an end at 31.7° and 0.16 px at 43.5°; the leaving frame still steps along the
  plank, and the carry of that frame stays as an x speed until the landing (+0x320); past an end
  the carry is that end's movement plus the turn about it. ✅ the Cat's ground sensor row
  (v3.0.3 rows 0x71023d497c.., pick 0x7101592480: pose 6 for form 7 in every ground state —
  head 15.9, walls 4…8.5 at 13.9): it walks / runs through a 1-tile-high tunnel (new course
  `3W_tunnel_form`); its landing frame keeps the air row's height with the wide reach; it rests
  8.6 in at the course's left side; no other 3D World form gets 13.9 (3W Super stops 7.9 from
  the tunnel, `3W_tunnel_items`); a move ending flush on a wall's right face zeroes the speed.
  ✅ the Cat's rolling attack (ClimbRollingAttack 34): unreachable in v3.0.3 — its only way in is
  the action delegate +0x2700 (0x71015e6f80), slot 60, which no state's action list (0x71015e76c0,
  161 callers) names. ✅ the view's side hold: 8.0 inside last frame's view in every style (not
  v1.0.0's 8.1) — the slow autoscroll in SMB1 … 3D World (new whole traces X_ascroll_<style>,
  `trace-courses` kind `autoscroll`) pushes the standing player 7.5 inside this frame's view;
  the autoscroll starts at 0.5 on the first update (a reset doesn't move it). ✅ enemy whole
  traces (`trace-courses` kinds wbros, boo, lavab, pbeetle, stop, bowser, bowserjr, walkin): the
  Lava Bubble rebuilt from v3.0.3 (0x71010a0570 / 0x71010a08b0 / 0x71010a0ee0: placed falling,
  home from its third falling frame, L = lava − 12 or the area's bottom, hidden at L − 32, the
  sine leap) — X_lavab_M1 exact 600 frames; the Boo sees the player a frame late (X_boo_M1
  exact); the Para-Beetle exact as it was (X_pbeetle_M1 / _WU); Spike Top corners: the outside
  turn's last frame doesn't move, its speeds are the step / 0.49 into the surface, an inside turn
  off a wall idles a frame first (X_stop exact to the player's death, 462); the winged Hammer Bro
  placed on the ground hops on its first update and its jump moves on the frame it's decided,
  hammers go 32 under the view (X_wbros_M1 exact to frame 242, then its random facing); a Spike
  Top meets the player 1 px narrower a side (10.5 hurts, 11 not; X_stop_M1 / _WU exact 600
  frames) and is put exactly on a face after a corner; the 3D World Goomba's Find is 45 frames
  and it stands 8 after a landing (X_walkin_3W, X_cpipeg exact); Bowser placed on the ground
  walks from frame 0 (X_bowser_M1 exact to his first random draw, 47); Bowser Jr's fireballs
  (SMB1) fly at 1.0 from the frame after their birth, one every 61 frames (seen in X_bowserjr_M1,
  whose Jr hops on a random draw at 20). Left: the game's random draws (Bowser, Bowser Jr, the
  winged Bro's facing); 3D World Hammer Bros / Bowser not found by the scans yet; the projectile
  budget can't be reached in an editor-made course (100 enemies an area).
  Coverage 548 / 550
  + 272 / 273 (as main); simulate 0 crashes.
- 🔄 **Physics pass 11** (2026-10-25, branch `physics11`, research specs/random.md, fidelity-log.md
  "physics11"): the game's random numbers. ✅ the generator (sead::Random, xorshift128, v3.0.3
  0x710029a150) is the play scene's, seeded init(0) at every course start (0x7100e489c0): no clock
  in it; ported as `sim/src/random.rs`, lent to the whole tick, part of `World` (snapshots,
  checksum; SIM_VERSION 3). ✅ every draw site of the Bowser / Bowser Jr / winged Bro traces
  logged in the game (a getU32 hook, `rcap2.py`) and matched one by one (`mmk trace --draws`):
  creation draws (the shared map-object init's three words for SMB1 … NSMBU actors, Bowser's
  150 + r(60), the SMW / NSMBU Bowser's decision objects), the player's Walk entry and moving
  frames, Bowser's fire helper ahead of his step, low-bit draws (the Bros' facing / jump).
  **682 / 683 traces exact** (was 675): X_bowser_M1 / _M3 / _MW / _WU, X_bowserjr_M1 / _WU,
  X_wbros_M1 exact (Bowser and all his flames over 601 frames in X_bowser_M1; Bowser exact in
  re-recordings with his object for SMB3, SMW and NSMBU, 601 frames; both Bowser Jrs exact).
  ✅ found under the draws: SMW / NSMBU Bowsers take their counts from the decision objects and
  never arm the breath helper; NSMBU's Fire state runs two frames longer, its flame (0.9 px/f,
  level on its birth frame) leaves from the mouth joint and aims from there; Bowser Jr's
  GoStraight fireballs (and the SMW / NSMBU straight flames) meet the player within 5 + 4 px and
  hurt a frame later, like an enemy's touch (a boss's body too); the big jump's take-off and
  SMW's jump out of LandOn; NSMBU Bowser Jr's fireball (hand joint, a frame later, 62-frame
  cycle, table aim) and his wind-up a frame past the clip; no re-seed on a restart; creation
  draws by kind (none for the Angry Sun, two for a Chain Chomp). ⬜ Left: X_bowser_3W (3D
  World's Bowser draws from the game's clock-seeded global generator and isn't modelled from the
  game; the engine runs NSMBU's rules on its own stream). ⬜ The audit (fidelity-log "physics11"):
  the game's draw sites for the walking Hammer Bro, Lakitu, Magikoopa, Rocky Wrench, Fire Piranha,
  Chain Chomp, Boom Boom, the Fire Clown Car are logged (new SMB1 `trace-courses` kinds), not yet
  ported; Wiggler / Angry Sun / Monty Mole agree; Koopalings, Pom Pom, the 3D World enemies,
  tower sway, block debris not traced. Coverage 550 / 550 + 273 / 273 (as main); simulate 0
  crashes on both.
- ⏸️ **Physics pass 12** (2026-10-25, branch `physics12`, paused with SMCraft; fidelity-log.md
  "physics12"): the physics11 audit's random users, recorded whole with their objects and every
  draw. 🔄 Ported but **not committed** (the gate couldn't finish: free disk under 3.5 GB; the diff
  is stashed in the worktree and saved as research/tools/ryujinx-trace/physics12/physics12-wip.patch):
  the walking Hammer Bro (Walk + the timed throw helper: X_hbro_M1 exact, all 27 draws) and the
  Lakitu on its cloud (X_lakitu_M1 exact to the death frame; a Lakitu's own cloud is no separate
  object; the Spiny's touch 1 px narrower) — 683 / 685 on the private copy. ⬜ Next: free disk,
  gate + cmpall, commit; the death-frame order (actors before the hurting enemy still move);
  Magikoopa, Rocky Wrench, Fire Piranha, Chain Chomp, Boom Boom, Fire Clown Car (recorded, sites
  logged); then the unconfirmed-order users, 3D World's Bowser, re-seeding, the Sledge Bro course.
- 🔄 **Inferred-behaviour pass 6** (2026-10-25, branch `infres6`, research specs/inf-inventory.md
  "Pass 6"): ✅ the Propeller Mushroom's own Move (#38, ItemPropeller 0x710134ca70 / 0x710134c3c0 /
  limits 0x710134c880; the Block context's enter 0x710134d550 skips the slow rise): out of a
  block 8 px up then 3.5 up (g −0.09375) through terrain, a frame's stop, the six (cap, gravity)
  pairs with the gravity's sense flipping at each cap, drifting 0.7 px/f the way it faces (the push
  always + in game-version-0 courses), climbing away at 4; the downward exit slides 2.5 held at
  1.8 (`actors/propeller.rs`; a placed one meets terrain [inf]). ✅ the other style power-ups'
  Move: the Leaf / Feather Move sets the terrain-off bit and has no ground case (0x7101347170 /
  0x7101346980) — out of a block they sway on through the floor (placed ones still land [inf]);
  in water the push flips at 3; the landing choice (actor slot 0x5e0): Super Bell / Super Hammer /
  Boomerang Flower 0x7101353970 → Wait, Big / SMB2 Mushroom, Frog, Balloon, Acorn, Leaf,
  Propeller 0x710134ef60 → Move (unchanged). ✅ the Fire car's ChargeExplode (0x7100ddf950): the
  burst's tiles (its tile moved toward what it hit, row and column ±16 / big ±32 + 3 × 3, the
  column behind) break as a blast's (the shared block hit 0x7100e22210). 🔄 the projectile
  budget: one 100-unit area manager (area +0x108: request 0x7100de9960, release 0x7100de9a60,
  queue promotion in the area update 0x7100e443c0) of throw reservations (1 unit for most
  throwers, 2 / 3 / 4 for a few), held from the throw's request to the state's exit — not live
  projectiles; its +0x8 term's writer not found; not ported (only reachable with dozens of
  throwers at once). ✅ Bowser's jump-height search hooked up: the type-0x25 class-0 units are
  the Trampoline's body (JumpStep 0x71013b9260, _INIT_369 "Jumpdai"); one over his columns
  within the jump's reach (apexes 0x71011530f0) stops the chase and big jumps and starts the hip
  drop at 1.0. ✅ #16: the Turn exe waits for the anim controller's end test, so the loop bit
  doesn't decide (the existing length / rate rule stands). 🔄 #15: the shell's own callback
  sends kind 10 / 5 to its reaction component on its +0x534 bits 19 / 20 (not followed). ✅ #17:
  the Bob-omb blast is the shell attack. 598 / 598 traces exact (merged with physics8); coverage 550/550 + 273/273;
  simulate 0 crashes.
- 🔄 **Inferred-behaviour pass 7** (2026-10-25, branch `infres7`, research specs/inf-inventory.md
  "Pass 7"): ✅ the projectile budget ported (`actors/budget.rs`): one 100-unit manager per area
  (built in the area ctor 0x7100e439d4 with a 200-entry ring; request 0x7100de9960, release
  0x7100de9a60, promotion in the area update 0x7100e443c0, release on deletion 0x71008d8530); a
  reservation is kept between throws, so it caps the throwers acting at once; its +0x8 term is
  only zeroed (ctor, reset 0x7100e4b8c8). Wired: Hammer / Sledge Bro timed throws, the winged
  Bro's WingWalk (released at its exit), Rocky Wrench, Magikoopa, the Fire clown car's charge
  (released at the Chase exit). ✅ placed Super Leaf / Cape Feather / Propeller start in their
  Move (base context 0x7101298b90 → Basic enter 0x71013466f0 with no event → Move), through
  terrain. ✅ Bowser (SMB1 / SMB3 action 0x71028e44b8): LittleJump vy 0.7 / 0.35 under a
  Trampoline within the hop's reach (0x7101158ff0), no middle jump under one within its reach
  (0x710115ab40); water reaches; the jump counters per style (SMW 0x7101157fa0 / 0x7101158030,
  NSMBU 0x7101152cb0 / 0x7101152d40). 🔄 #15: the shell's "blocker" branch is dead (nothing sets
  its +0x534 bits 19 / 20); the general branch not followed. 🔄 decoded, not ported: the Spike
  Top's RotationCliff (#24), Bowser Jr's shell-slide wall analysis (#37), the SMW / NSMBU Bowser
  decision tails. 598 / 598 traces exact; coverage 550/550 + 273/273; simulate 0 crashes.
- 🔄 **Inferred-behaviour pass 8** (2026-10-25, branch `infres8`, research specs/inf-inventory.md
  "Pass 8"): ✅ #24 the Spike Top's RotationCliff (0x7101278940) and contact probe (0x7101279310):
  0.34 px/f (big 0.72) along θ + 5°σ while the body turns 3° (binary exact, 31 frames; 1.5° / 61
  and the move halved under the water-speed rule), the probe at the feet + R(θ)(σ(6 − 10q), −4)
  (big σ(8 − 12q)) drops it when the corner goes; the Walk's ground test is the ray along the feet
  (0x710231c300); the push-out onto the new face is [inf]. ✅ #37 Bowser Jr's ShellSlide wall
  analysis (0x710108aa50): wake-up rays, the hop, the arc search for landing heights 16 … −128,
  the ledge look under his front edge (pits, lava). ✅ SMW / NSMBU Bowser's own states
  (actors/bosses/styles.rs; KoopaMW 0x71028e4060, KoopaWU 0x71028e3648): Walk cycles and speeds,
  LandOn lengths, SMW Little / MiddleJump with the in-air fire, Fire + Wait, FireFalling and its
  rain manager (0x710115c660), BigJump; NSMBU BeforeJump / Jump, Fire (aimed, 0x710114b470),
  JumpFire, BeforeBigJump / BigJump, SpreadFire; the chase's gap jump as the big jump. 🔄 #15:
  the general branch (LAB_7101201db4) sends the shell's own reaction 0 unless +0x52c bit 31; the
  branch taken depends on the boss collider's data flags (+0x91 bit 0, +0x92 bit 6), not traced.
  🔄 #36: the eat handler (+0x338 → +0xc88, vtable 0x71028e8508) forwards to the actor's vfuncs
  0x4a8 / 0x4b0 and context switch 0x7101299930; the item overrides not followed. 598 / 598
  traces exact (40 new physics9 recordings since pass 7, 37 of them diverging on main too, not
  counted); coverage 510/550 + 211/273 (sound-effect blockers, as main); simulate 0 crashes
  (23 / 21 cleared).
- ✅ **Inferred-behaviour pass 9** (2026-10-25, branch `infres9`, research specs/inf-inventory.md "Pass 9"): the
  last three sound-effect parts (Seesaw, Tree, Dry Bones Shell; coverage 550/550 + 273/273); #15 a sliding shell
  slides on through a boss (its own callback leaves an enemy-class shell alone; a big shell costs Bowser 5; [inf]
  once a contact); #36 power-ups are held in Yoshi's mouth and spat into the item's EatOut (eat type table
  0x71023cb894, catch 0x71015fdc10, spit 0x710134f870); #22 the tongue tip per style from the eat animations at the
  anim table's rates (NSMBU 75.4 px in 7 frames, SMW 53 px from frame 9 of 17). Left: Bowser's rain re-anchor
  (condition located, values unidentified), the aimed flame's mouth joint, Bowser Jr's wall bit 16, #3. 652 / 654
  traces; simulate 0 crashes.
- 🔄 **Placed sound effects** (2026-10-25, branch `separts`, research specs/sound-effect-parts.md):
  ✅ the record (spots: id, tile x, tile y, 0, 300 an area; a part's `sound_effect_id`; Mario's
  in header byte 0xF0, now `player_sound_effect`: the course writer 0x7100fde2c0) and its round
  trip; the 47 offered course ids, labels, the window's five pages (the rows of the course icons'
  sheet) (icon → id 0x71023ca370, labels _INIT_1888; `mmk_formats::smm2::sound_effects`). ✅ in
  play (`mmk_sim::otoasobi`, an observer the hosts step after each tick, no world hooks): spots
  by a player's box against the spot's centre, once an entry (0x7100e443c0); parts by what they
  do — enemies defeated, coins / items taken, blocks hit, Springs bounced, pipes / doors entered
  (the play calls' trigger kinds 1 / 5 / 6); Mario's effect at the start and on its action
  (running jump ≥ 3.0, damage, a fall out of the view, power-up, getting in, a skid:
  0x71015d4900–0x71015d4fa0); the voices (16 shared, one long, own with hold: the table
  0x710294f468) and the music effects' start range (an object's 160) and ends (a spot's 160,
  an object's 180: 0x7101537ab0, 0x7100e443c0). ✅ the host: the effects' `cue` sounds, music effects
  in the theme track's place (`cue_bgm` / `_hurry`, the `_Link` / `_USA` users), the music-like
  ones (Heartbeat, Silence, Peaceful, Horror) over a stopped track [inf], the pictures (the
  effect users of the effects import). ✅ the maker: the palette's sound button, the effect window
  (`L_Category_01`), putting effects on tiles / parts (EditDB `isCanContainOtoasobi?`) / Mario,
  erasing, the course icons (`Otoasobi_CategoryPanel`; sizes [inf]), `EditOtoasobi<id>` sounds.
  Importer / game pack 17 (the Otoasobi sound users), UI importer / pack 10 (the pictures) —
  **re-import both**. `mmk coverage` counts what isn't played: the two audio filters (Twisty
  Turny, Woozy: DSP on the whole mix) and effects on parts whose action isn't modelled (P Switch,
  POW, sideways Spring, Muncher, warp box, lifts, Key, Bill Blaster, claw, Mushroom Trampoline
  …): 510 / 550 and 211 / 273 (228 / 550 and 126 / 273 courses use effects). Left: those parts
  (the kind-4 callers 0x71013924e0…), the filters, the spot's ground drop, the icon sizes, the
  hold rule's voice lengths. Captures: `maker-sound`, `maker-sound-ring`, `maker-sound-icons`;
  in play, the courses in the spec's list.
  ✅ **2026-10-25, branch `separts2`**: the audio filters (spec §2b: the OtoAsobi bus — Twisty
  Turny's delay + resonant band-pass swept 60 → 5000 → 50 Hz at Q 1.8 with the bus at 2.5, Woozy's
  vibrato + tremolo by a level that rises on a player's entry; 16 beats of the course music's bpm
  from `Mush/BGMList.byml`, then the `end` scratch; 0x71017b15d0, 0x7100e443c0, the DSP
  0x71004fe0a0 / 0x7100521700 / 0x7100520780; `mmk_audio::oto`, checked with `mmk audio-render`:
  16 beats at 100 bpm end at frame 576, the band-pass peak follows the sweep); every part's
  trigger by class (spec §4's table: 0x7100d84810 is the shared "stood on", not the pipes — pipes
  are the warp sequence's kind 5; P Switch, lifts, snake blocks, Dash Block stood on; Key, claw,
  ! Block kind 4; Bill Blaster / Cannon shots, cars / Cloud got into kind 1; Yoshi's Egg hatched;
  moving blocks; the POW and Crate only as music sources); music effects of parts as the source
  registry 0x7101538eb0 (while alive, following, 200 px in, 304 out by kind); the course track
  plays on under Heartbeat / Silence / Peaceful / Horror and resumes after a music effect (the
  BGM layers 0x71017a6f20 / 0x71017a5990); the spot's ground ray; Mario's effect on a respawn
  (the player's create virtual); the hold by the sound's real length; the maker's sound button
  balloon (`Mode_Disp_*`) and the window not relaid as the parts ring. Importer 18 / **game pack
  18** (`sound.json` `tempo`, `otoasobi_bus`) — re-import. Coverage 548 / 550, 272 / 273 (left:
  Seesaw, Tree, Dry Bones shell); simulate 0 crashes. Left: those three, the default sound palette
  (✅ 2026-10-25, `maker3`: the tables 0x71023d8508… are the effect window's eight pages, now
  applied; the palette's start still [inf]), the icon sizes, the put-down sound's moment (✅
  `maker3`: 0x7100f0eaa0 is the effect icon's put-down virtual), player states
  144 / 145, the console delay's network.
  ✅ **2026-10-25, branch `infres9`**: the last three — the Seesaw (its physics 0x71013d7d90 plays
  through the shared stood-on call when a player lands on its empty plank with its 25-frame
  count out; the count refills on every such landing — this was read as the P Switch's, whose
  only call is its pick-up 0x71013e99a0), the Tree (0x710137eae0: each time a player reaches the
  top of the trunk, at 40 px above its origin, kind 5) and the Dry Bones Shell (an enemy of the
  enemy base). Coverage 550 / 550 and 273 / 273; simulate 0 crashes.
- **Agent queue** (when a helper slot frees; running 2026-10-25: the maker's Water Level and
  Custom Scroll modes, physics): ✅ a Koopaling in a Clown Car (2026-10-25, branch
  `kclown`, research 120-koopalings.md "In a Koopa Clown Car": KoKoopaDefault's Clown vtable
  0x71028b6120 shared by all seven — Lemmy / Roy override only the setup; Move fires the
  controller's timed volleys, a hit stops the car (KoopaJrStop) while he ducks in and comes back
  in his shell for 300 frames, HP only in Move; tgr2 273/273; left: NSMBU's shell yaw spin); ✅ the
  Fire Koopa Clown Car's own fireballs with an enemy rider (2026-10-25, branch `enemysys2`,
  research 42-koopa-clown-car.md "Pass 5": the car's fire helper 0x71028e83f8 / record, cooldown
  90 then [90, 180), a 90-frame charge braking the car, one KoopaClownFire EnemyShot at 1.5 px/f
  plus the car's speed; bosses in a Fire car too; a player flying it fires PlayerShots on each
  change of Run (cooldown 12 / 50, ≤ 8 out, three spread as Fire Mario [inf]); ✅ the
  ChargeShot's flight (branch `blockout2`); left: its ChargeExplode burst, the object budget); ✅ own Parachute falls (2026-10-25, branch `enemysys2`, actors/chute.rs,
  shared enemy draft §5.2 "Traced"): the Thwomp's Dossun action folds its parachute and opens it
  on each drop (vy ≤ −4: −0.1 / −1.0), the Lava Bubble's ends in lava or at its hiding level, the
  Cheep Cheep's and Porcupuffer's in water or below the area, the Wiggler's is the Default (its
  segments' rotation is drawn only); ✅ the Side Thwomp's (SideDossun Fall 0x7101265210, branch
  `blockout2`); ✅
  ceiling Spinies and Buzzy Beetles (2026-10-25, branch `enemysys2`, actors/ceiling.rs, 25-spiny.md
  "v3.0.3 check": placed upside down — 58 in the sample — or out of a down pipe at normal size,
  they walk the ceiling and drop as their shells on a player below; Togezo Basic 0x7102913c70);
  ✅ Boo Buddies out of a block (TeresaSpread Block Out 0x71029106e0: the circle spreads round the
  block; 41-boo.md); a block's part keeps its 0x4 form (lit Bob-omb, Sledge Bro …); ✅ note blocks'
  contents rule checked (v3.0.3 0x710138f990, 23-note-block.md: given at bump start on a side hit
  or when the block is in a camera view, else kept for a later bump); ✅ FishBone's Block Out
  (Pukupuku's, 103-fish-bone.md) and the Chain Chomp's pipe exit ending on landing (Wanwan Dokan
  0x7102915ff0, 61-chain-chomp.md); ✅ water variants (shared enemy draft §5.9): a Goomba out of
  a block in water holds still, then swims; a Lava Bubble leaps 3.5 (5.0); Fish Bones fired in
  water glide level and slow to a stop, Wigglers swim off at once; the winged red Koopa's climb
  eases by 0.06 out of water (the speed bands were its water variant); ✅ Gesso3W's Block Out
  (collides from the start); ✅ the rest of the Block Outs (2026-10-25, branch `blockout2`,
  shared enemy draft §5.4 / §5.9 / §5.10): ItemBalloon / ItemHeaddress eased out (the
  P-Balloon and the wearables: 63 in 7 tgr2 courses), the Coin's (winged / parachuted coins),
  Hanachan's and Togezo's water variants; the winged coin out of a side pipe (the Coin Dokan enter's state 8 is EnemySys
  context 8, Killer: v3.0.3 context list 0x7101286ee0); the Side Thwomp's parachute (folded,
  opens in each charge at |vx| ≥ 2.3: 0.1 / 1.0; 36 in the samples); the Fire car's ChargeShot
  flight (4.0 px/f, no life count); left: the ChargeExplode burst's hit kind 2 on blocks
  (0x7100e22210 → 0x7100e258d0), the 100-unit projectile budget (0x7100de9960: what fills its
  +0x8 / +0x670 counts, for every shooter), Coin10's Block Out, Block sideways (multiplayer)
  variants; ✅ the Super World map's music, sounds, point / goal / road states and ledge cliffs
  (2026-10-25, branch `swmap2`, game pack 14; left: `passPoint`, the camera, ghosts are online); ✅ the Cursed Key and Phanto (2026-10-25,
  research 95-key.md; co-op: Phanto hurts only the player in front [inf]); ✅ SMM1 UI sounds for
  the SMM1-look 100 Mario (2026-10-25, branch `smm1snd`, research smm1-sound.md: SMM1's SLink2DB
  v0x15 decoded; the import's `ui/sound/`, SMM1 importer 4; the screens fire the data's keys and
  states; left: pitch units, by-ear check against the game).
- **Waiting on the owner:** the SMCraft app on Nextendo's portal (public client / PKCE, scopes
  `identity game.matchmaking`, redirect `http://127.0.0.1:47614/callback`) and its client ID; the
  game-server access key — (a) read from the player's own dump at import (recommended) or (b) one
  Nextendo issues. Nextendo has agreed to SMCraft going online. Releases need the owner's OK.
- **Next, in order:**
  1. Physics (352 / 352 exact over 26 courses, enemy stomp / die included; the trace Ryujinx
     boots again from /private/tmp copies — a macOS privacy prompt hung it on ~/Documents and the
     SSD): the 3D World courses (a MoltenVK build without 3D texture compression is ready,
     `ryubuild/no3dbc`), Koopa / shell traces, WU form. Pixels (3D World mean 46.0 → 29.4;
     NSMBU day 38.5 → 17.8, night 39.9 → 14.6; NSMBU's TEV stages ✅, its background's place ✅,
     the light-map domain ✅ (wrapped, with the light map's pow; needs re-import, pack 13), rim light ✅,
     the backgrounds' texture-matrix scroll ✅ (pack 15, NSMBU 17.8 → 15.0, nights 14.6 → 9.2)): the
     NSMBU sky's cloud layers' place, specular / env-map reflections, baked AO (needs an importer
     bump), the other sky variants, 3D World snow, the colour table, 3D lava.
  2. Maker: ✅ wings / parachutes (the attribute balloon, merged 2026-10-24), ✅ over a multigrab
     selection (hold a selected part: wings / parachute go to every part that takes them, one
     undo step); a part put into a block keeps its wings / parachute / form / size as the
     contents' flags ✅; the play side ✅ (any part out of a block, dressed: `Contents::Part`,
     2026-10-24); left: NSMBU / 3D World wing art, the contents balloon (`Edit_BalloonPartsIn_00`, page state 6 `cDispInParts`,
     request bit 0x800 — maker.md; what opens it: one Ryujinx look, with the trash can and Water
     Level mode); tracks and links.
  3. ✅ Co-op players screen: none to build — SMM2 opens the system's controller applet
     (`nn::hid::ShowControllerSupport`, states `cRunControllerApplet` → `cPlayTogether`); SMCraft's
     controller step stands in for it.
  3b. Research queue (binary → spec → code), each an agent's task: stacking's tower rules and
     `Tower_stack` (maker C11); ✅ the maker's Water Level mode (2026-10-25, branch
     `makermodes`, maker C13: the v3.0.3 edit actor 0x7100eff460 — handles at 31/48 of the view,
     the end's round button ±8 px at the anchor − 16, the start's pill ±24 × ±8; dragging sets
     the heights 0 … 24 (from 1 in 3D World, 167 vertical); lines together = still (mode / speed
     saved, 1 / 1 at first); a tap on the button 1 ↔ 2, held > 15 frames the speed balloon;
     built: banner, button states, lines / pill / path / button drawn, the balloon, the first-time
     tip) · ✅ the water-level actor's sound user (`<style>_EditLavaEditor`), the eraser greyed
     in the mode (2026-10-25, branch `makerart`);
     ✅ custom scroll nodes (2026-10-25, maker C13: the start / end units, a tap on a point's
     speed square at + 16 px steps 0 → 1 → 2 → 0 (0x7100eb3300), the start's speed is header
     0x0E; built: Custom in the autoscroll window, its tip, the mode with the band, birds, speed
     squares and arrows; nodes on their own layer, numbered left to right) · ⬜ how the game adds
     nodes (ours: drag a leg), the band's exact rule (ours since `makerart`: the camera-centre
     range grown 2 tiles across, 2.5 up / down, as the frames' left and bottom edges); ✅ the
     modes' art (2026-10-25, branch `makerart`, maker C5 / C13: UI importer and pack 8 —
     **re-import the UI pack** — copies
     `Edit_WaterLevelLine`, `Edit_dragBtn`, `Edit_EditPanel` (the 13 × 6 icon sheet),
     `Edit_autoscroll`, `Edit_camera`; the lines, pill, path marks, round button (1.18 × held),
     drag arrows, birds, speed squares and scroll marks placed as 0x7100effd00 / 0x7100f01ce0 /
     0x7100eb27e0 / 0x7100fd5c10 do; the markers at the path's unclamped points; faint lines /
     birds outside the modes; the banner is the layer group's; SMM2 layouts' colours taken as
     linear (`FLAG_LINEAR_COLOR`: the yellow buttons), window frame flips as texture turns, the
     tip's dim 200) · ⬜ the scroll bar along the bottom (the frames show only the area button in
     the modes; ours covers the pill), the game's maker camera height; ✅ Mario's Trail (`L_ZanzoBtn_00`, 2026-10-24:
     maker C12 — 40 snapshots of player 1, one per 22 px of travel; built), the trash can
     (`L_TrashCan_00`: UIEditMainPage part 8 at +0x800, set up in 0x71018cee20; `part(i)`
     0x71018fc060 is virtual; placed from code, the pane sits at 0,0; Act 45 plays
     `se_ui_edit_trash` + `SE_DropErase`; a look in the recomp (2026-10-24, touch drags by script) shows no bin while a
     part is dragged in the main maker, to the corner or past the palette: it may belong to another page
     (`_INIT_1878`'s five-slot palette page) [?]);
     ✅ winged / parachuted items and any part out of a block (2026-10-24); ✅ coins from a Bill
     Blaster and the fired parts' launch (2026-10-24, branch `blaster`: the specs' addresses are
     v1.0.0's (the decomp corpus); in v3.0.3 the Coin Shot enter is 0x7101366c30, the blaster
     0x71011406f0 / 0x7101141e20. A coin blaster fires a TenCoin burst of 10 Coin actors, each
     preset's Shot is in actors/shot.rs; left: coins from a Lakitu (Coin Jugem `Thrown`
     0x7100847c20 is read, but after 50 frames it hands a coin over to the Wing context, whose
     Move for an unwinged coin isn't traced)); ✅ the entry contexts (branch `contexts`,
     2026-10-25, v3.0.3 addresses throughout): Block Out per preset (actors/block_out.rs:
     Default, RedPataPata, PataTogezo, FloatSearch, Pukupuku, Bubble, MapObj; Default vtable
     0x71028b2190), Dokan per preset (actors/dokan.rs: parachute, winged RedPataPata /
     PataTogezo, DokanOutBig → JamBig → JumpOutBig, Bubble launch, item facing, coin pipes as
     TenCoin bursts; init 0x710105e400), Killer leftovers (Togezo / Met / Karon, empty shells,
     Bubble, MapObj = Item, Hacchin = CloudShot; the red blaster's 320-frame lock-out; Bullet
     Bill at the muzzle's height), Parachute swing direction and wall hold. Left: ✅ liquid
     variants (Kuribo / Pukupuku / Bubble / FishBone / Hanachan shots / RedPataPata in water), Block
     sideways (multiplayer) spreads and ✅ note blocks' contents, Gesso3W / Hanachan / FishBone /
     ✅ TeresaSpread / FishBone / Gesso3W / Hanachan / Togezo / ItemBalloon / ItemHeaddress /
     Coin Block Outs, KutsuKuribo, ✅ Togezo's ceiling mode, ✅ the winged coin's side-pipe exit
     (context 8 = Killer), ✅ Bubble / Thwomp / Porcupuffer / Wiggler / Cheep Cheep own Parachute Falls, the
     parachute body sway (drawn), DokanOutBig's 0.5 → 0.7 scale and other draw-only scales;
     ✅ Super World enemy-icon courses, point icons by model (branch `swmap3`), map ghosts; ✅ CJK course
     names (branch `cjk`, 2026-10-24: the
     UI pack keeps the decrypted scalable fonts (`ui/scalable/`, 31 MB) and each baked font's
     recipe; a character the sheets lack is rasterised on first use at the baked font's metrics
     onto shared 1024² glyph sheets (`mmk_ui::glyphs`), which the portable shell and the Swift
     app upload again when they change; Chinese / Korean fonts as the game's `_CNzh` / `_TWzh` /
     `_KRko` combinations add them; UI importer and pack 6 — **re-import the UI pack**). Left:
     the region combinations' range modes 3 / 4 / 9 (not parsed; their fonts are tried after the
     combination's own), the web build reads all 31 MB of fonts up front.
  4. Super Worlds' map ✅ drawn with the game's models (branch `swmap`, UI pack 7 — **re-import
     the UI pack**): left the ledges' cliff faces, map ghosts, the goal's appear / cannon and
     Passable states, the map music; airship sway; the colour table; NSMBU / 3D World pixels.
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
- ✅ CI on the public repo (`tests.yml`: Linux on every push, superseded runs cancelled; macOS /
  Windows / wasm by hand; `no-proprietary.yml` on every push; `release.yml` on a tag)

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
  folder with the NSPs and `prod.keys`) ✅ run end to end in headless Edge 2026-10-25 (36 s, the full
  pack and the menus; it had lost all but the effects' 276 textures: the worker couldn't read back
  `textures/index.json`); iOS / iPadOS 🔄
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
- ⏳ Popular across difficulties / more than 20 New courses: TGRCode can't — `search_new` caps
  at 20 with no paging, `search_popular` takes one difficulty and the client is held to one
  request per 2 s (four lists ≈ 8 s); Popular stays TGRCode's Normal ranking
- ⏳ Leaderboards / Ninji: TGRCode serves no rankings or Ninji ghosts (only our own server has
  leaderboards); Super Worlds screens ✅ (G5)

**G3 — Challenges** 🔄
- ✅ 100 Mario Challenge (SMM2 and SMM1 presentation, rules from SMM1)
- 🔄 Endless Challenge: rules ✅; SMM2 screens on SMM2's own rules ✅ (`front/src/endless_run.rs`,
  G7's Challenge Modes); the server's runs and rankings ⬜ (TGRCode is read-only); courses from TGRCode ✅
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
  - ✅ No players screen in SMM2's layouts: the game uses the system's controller applet
    (`ShowControllerSupport`); ours is the controller step · ✅ the P-door hold for followers (§7.5, 182 [inf])

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
    front of the palette · ✅ the game's pages and order per style (2026-10-25, branch `maker3`;
    maker spec C17: the ring reads the 2P menu's tables 0x7101922b20 / 0x71019228e0, a page's
    category by its first page), each page keeps its selection (+0x16b8); the effect window's
    eight pages (0x71019227b0), a new selection sounds its effect (0x71018e0c40) · ✅ the Spike
    Trap's, Hidden Block's and Clear Pipe's pictures (were blank) · ✅ the course view draws
    Hidden Blocks as see-through ? Blocks (`World::edit_view`, `Sprite::see_through` at 0.75;
    3D World its `3W_Object_block_tomei` mesh) and Spike Traps in play and in the maker
    (`<S>_Field_anime_toge_<set>`, the field's animation tiles [obs v3.0.3 0x7100d9add0]) ·
    ⬜ captures to compare (smm1lib's report)
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
    ✅ multigrab selections (2026-10-24), contents keep their flags · ✅ the "what is
    inside" balloon (2026-10-25, branch `maker2`; maker spec C16): the same 16-tick hold on a
    part holding something shows `Edit_BalloonPartsIn_00` (`InBig_00` for a big content) with
    the content's picture and its dummy wings / parachute instead of the attribute balloon
    (0x7100e9e400 first; player 1 only, not on a track), carrying it on closes it (0x7100f9fe00),
    both balloons on the game's side (0x71018bb490, 0x71018c4450) [inf: letting go closes it] · ⬜ wings on the NSMBU / 3D World models
    (their own textures; SMB1 art stands in), parts that drop wings / parachute with their
    other form (vtables not resolved), winged Bill Blasters in play
  - ✅ Mario's Trail (2026-10-24; `research/specs/maker.md` C12, from the v3.0.3 binary: the
    game's `SnapshotTextureMgr`): during test play a picture of player 1 each time it is more
    than 22 px from the last (none below the area's bottom), at most 40, each the 160 × 64 px
    round the feet (`Host::player_snapshot`: the hosts copy the player's sprite out of its
    texture); back in the maker the short / screen-bound part is trimmed as the game does
    (ends < 128 px apart: last 12; 27 within a screen: those), then they come in one every 2
    ticks, oldest first, alpha 0.3 → 0.7, under the grid. Emptied by a new test play, an area
    change (in play or the maker) and a style / theme change, kept through edits. The left
    column's `L_ZanzoBtn_00` (Check = on, `se_ui_edit_open_ringmenu`, `yr_Delay_01` switched
    on) toggles it, kept as `maker.trail`. ⬜ 3D World's model player (no 2D art: no pictures),
    what Mario holds / rides in the picture, the maker following Mario to the area he ended in
  - ✅ water level and custom autoscroll with its nodes (2026-10-25, see the research queue) ·
    ✅ the part variants (2026-10-25, branch `makerparts`; maker spec C10): the balloon offers
    every form, stored where the game keeps it — the 0x4 bit, or (flags >> 18) & 3 for the pipe's
    colour, semisolid / mushroom looks, (slope) conveyor speed, door kind, big coin value, spike
    block mode, Mechakoopa kind (0x71003d2600, 0x7100d80df0) — a tap steps to the next form, a
    door / warp box pair changes together, SMB1-only and 3D-World-only alternates by style ·
    ✅ a door / warp box brings its partner a tile to the right (0x7100ec9c00); with that tile
    taken the pair is refused, nothing moves it (0x7100eca770; 0x7100e9b840 is the grid snap;
    2026-10-25) · 🔄 the trash can (2026-10-25, branch `maker3`; maker spec C17): its controller
    built (`maker::trash`: shown with In, the 45-frame `Act` with `Anm_Act_TrashCan`'s
    `se_ui_edit_trash` + `SE_DropErase`, Out once Act ends — the game's `UIOneTimeAnimCtrl`
    0x7101b4d640 / 0x7101b4d930 / 0x7101b4d830; capture `maker-trash`); ⬜ what starts it: no
    caller of the page's +0x800 or the controller's Act in the editor / UI code, no code asks for
    the parts' `dropErase` key; the layout is also the Super World maker's and Story Mode's
    assist editor's (`Edit_SupportEdit_00`, `UIEditAssistPage`) — a capture to take ·
    ✅ co-op making (2026-10-25, branch `coopmake`; maker spec C16,
    `maker::coop`): `L_MultiEditBtn_00` → `Edit_MultiEditWindow_00` → with two controllers (the
    system set-up asks for exactly two, 0x7100e363a0) both cursors at the view's middle ±16 px
    and the undo history emptied and off (0x710103e490; every history call skipped in edit mode
    2); player 2's green glove (`Edit_mainCursor2P`, import group `EditModel2P`), pad only, its
    own part, drags, erase and put through the same edit code (the cursors trade places);
    `Edit_PartsMenu_00` by its glove with the game's order (12 pages / 3D World's 11 of palette
    ids, 0x7101922b20 / 0x71019228e0; the ids' names from 0x7102cd5e08), steps across pages and
    round, categories by their first page, the `PartsMenu2P_*` sounds; a part one cursor holds
    isn't grabbed (0x7100f99260) or erased (0x7100fb57f0) by the other, its index followed
    through the other's edits; balloons player 1's; co-op test play for two players (play mode
    4, 0x710103f540); hosts hand player 2's controller over (`FrontInput::pad2`,
    `mmk_front_coop_pad`) and leave it out of the menus; captures `maker-coop`,
    `maker-coop-on`, `-menu`, `-carry`, `--front-script p2:…` · ⬜ the camera with two cursors
    (not found: the view follows the moving cursor, the other is kept inside [inf]), which host
    buttons stand for the Joy-Con's SL / SR / menu button, icicles in the 2P menu (no icicle
    part yet)
  - ✅ the vertical scroll bar for vertical sub-areas (`N_SlideBarV_00`: its handle by the view's
    height, pressed / dragged to scroll; captures `maker-vertical`, `maker-subarea`)
  - ✅ the sub-area's orientation window (`Edit_SubAreaWindow_00`: Horizontal / Vertical) when
    the maker goes to an empty sub-area [inf: when the game asks]; one undo step
  - ✅ more parts in the part window (2026-10-24): Spike, Pokey, Piranha Creeper, Mechakoopa, the
    seven Koopalings, P Block, Dash Block, the SMB2 Mushroom (SMB1), the wearables and the
    ON/OFF Trampoline (3D World); each checked placeable in every style that offers it (a test)
    and playable · ✅ the Clear Pipe (2026-10-25, branch `makerparts`; maker spec C14): drawn
    from a corner (a 2-tile pipe 24 px out in the drag's quarter, turning with it, taken back
    within 16 px; 0x7100fa9090), its end on / back (a bend at a time) / round a bend toward
    the cursor, from either mouth (0x7100ec1e90), carried by its middle, erased whole, 100 an
    area, nothing in its tiles; records encoded byte for byte (all 208 sample pipes); drawn
    with `3W_Object_dokan_tomei` (unit / top / curve00, turned) in the maker and play; capture
    `maker-clearpipe --style 3W` · ✅ the Clear Pipe's picture (2026-10-25, branch `maker3`:
    its unit + top meshes drawn on the CPU, `mmk_host::icon`) · ⬜ multigrab of clear pipes (✅ the ride in play: Phase 9, 2026-10-25); Lakitu's Cloud isn't a part of its own (no name
    in the game's part list: it comes with Lakitu, parts go into it)
  - ✅ tracks (2026-10-24): the Track part in Gizmos; a press lays a horizontal piece, dragging
    draws on from its end, 2 tiles a piece, 8 ways [inf: the gesture]; a part that rides tracks
    put down on one is attached (flag 0x400); a press on an end opens / closes it; erasing,
    undo as for parts · ✅ drawn by the game's rules (2026-10-25: a press lays nothing, a
    straight / 45° piece per step — no bends while drawing, 0x710231bb60 — on from a free end
    keeping its open state, back over the end's piece takes it away; 0x7100fafe40,
    0x7100fb0200), ✅ the track art in play (2026-10-24: a cell's
    template code is its field-atlas tile, `Tracks::art`; left: junction cells (0x2xx codes) keep a
    line, 3D World's tracks are models, and the maker's own track drawing) · ✅ joint codes as
    the game writes them (2026-10-25, maker spec C16: the caps' join table 0x71023c74f0 /
    0x7100da2060, the joint on the left / lower piece, 0x7101016610; all 3823 joints of the
    sample; type 2's k0 is its lower-right end)
  - ✅ pipes turned and stretched by their mouth (the cursor round the base: the longer axis
    turns it, the distance is its length [inf]; `Editor::turn_pipe` keeps the base tile)
  - ✅ maker music and part sounds (2026-10-24; `research/specs/maker.md` C9): the style's
    `BGM_<Theme>_Edit` for the area's theme while editing (a theme / style switch changes it,
    test play stops it before the course's own starts, back it plays again, leaving the maker
    stops it); parts' `<S>_Edit<Part>` sounds: put down (+ the part's voice [inf]), picked up,
    dropped, erased, refused (`partsMax` / `invalid`), put inside (`Block_in`, `Dokan_in`,
    `Killer_in`, `Jugem_in`, `Clown_in`, `Cloud_in`, `Key_In`), made big (`scaleChange`), form
    change (`p_AttributeChange`), multigrab frame / grab / drop / erase. Host hooks
    `edit_music` / `edit_sound`, routed in the shell and the FFI (so the Swift app has them) to a
    maker `Director` of their own. ✅ `move` per tile while carrying [inf: when], the start /
    goal drags (`<S>_EditGroundStart` / `GroundGoal`: drag, move, set) and stretching (the part's user), wings / parachutes
    (`voice_getWing`, `lose_wing`, `p_EquipPara`, `p_RemovePara`), the multigrab frame's
    `cursor` per tile [inf: when] · ✅ `Tower_stack` and stacking (a part dropped on a
    stackable one; 2026-10-25: only parts that ride nothing and face the default way, the
    drop handler's 0xE038601 test, 0x7100f5ad90) · ✅ a member erased: nothing moves, the
    columns are rebuilt from what still touches (0x7100f5c8a0, 0x7100f5ce60; 2026-10-25) · ⬜ a
    listen on hardware against SMM2
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
  ✅ CJK glyphs in names (run-time glyphs, branch `cjk`) · ⬜ moving slots, the Super World
  filter, the download flow from Course World
- 🔄 The Swift app: its host has the maker and Coursebot hooks ✅; a finger / mouse held down
  reaches the maker (paint, carry, erase) ✅; the maker keys from its keyboard ✅ (the portable
  shell's: U / ⌃Z, Y / ⌃⇧Z, Q, [ ], T, E, G, M, C; macOS and iOS hardware keyboards); the 3D
  styles' still view is flat as in the portable shell ✅ (the engine's `editing`: the scene's field
  camera is the orthographic one while the maker shows the course)

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
- ✅ The map with the game's models (`mmk_front::swmap`, UI import stage `myworld map` →
  `ui/myworld/map.json` + the models' textures, UI pack 7): the theme's backdrop (`MyWorld_DV_*`),
  the field built as the game's builder does (11 × 6 positions, the 47-piece water set by land
  neighbours, the "nothing area" set of the Main themes), road pieces by cell type (`_s` on water,
  curves, bridges, ledges turned with their ladder up, decorations A / B / C), point models by
  icon and theme with Wait / ToCleared / Cleared from `WM_PointPartsAnime`, pipes, Toad Houses
  (used ones cleared), the castle / last castle / airship goal, the player walking / swimming /
  flying / climbing by `WM_PlayerAnime` with N / S / side views. Row 0 is the map's bottom row
  (the field builder's art; up walks to row + 1). Captures `superworld-theme<n>`,
  `superworld-json:<file>`.
- ✅ The map's music, sounds and moments (2026-10-25, branch `swmap2`; spec §7.4–7.8): each
  theme's `BGM_WorldMap_<Theme>_Play` (main+0x294bb50), the `MyWorld*` SLink users in the game
  pack (importer / pack 14) played through `Host::map_music` / `map_sound` (the maker's director:
  keys and the data's action triggers on the models' animations, `@point` for the course point's
  kind); a clear plays the point's ToCleared, the player's jump, then the reveal (roads hidden
  until reached appear, points pop `Passable`, the goal rises `AppearGoalPoint`; `wayAppear`,
  `courseAppearEffect`, castle sounds); a used Toad House's ToCleared; a world's goal: castle
  crumbles, the cannon's prepare / launch (`nextWorld`), the next world's landing; unreached
  points stand still; the goal model by world / water / course (0x71014437e0); the ledges'
  cliffs from the atlas rows 6–7 [inf placement]. Captures `superworld-clear`,
  `superworld-worldclear`. ✅ the ledges' cliffs as the game draws them (2026-10-25, branch
  `swmap3`, spec §7.3: the ledge's land flags 0x7101445230 — land above / at its own cell from
  roads, the map's edge and empty cells in Main themes, joined ledges —, its draw 0x7101444b20:
  full land under them, face over land or meeting water, top, joined by same-kind ledges; the
  "B" ledges (flag & 3 = 1) from row 7; the field's land asks the same flags). ⬜ map ghosts
  (online: other players' runs with name balloons)
- ✅ The walk and the camera (2026-10-25, branch `swmap3`; spec §7.7, §7.9, read from v3.0.3):
  the route graph's walk-through flag (0x710147b9a0: curves always, roads with exactly two
  reached neighbours; every point, pipe and Toad House stops) — `passPoint` is the stop's sound
  on a course, the goal or a Toad House (`sw::walks_through`, `plays_pass_point`); 1.5 units a
  frame, 0.75 on ladders; hops onto / off pipes (12 up, the game's arc) and stands on them; A
  enters only a course not cleared / a house not used, after the player's `coursein`; balloons
  only there. Enemy icons (8–15) start their course on contact (10 units, on arrival walking
  down the screen; `sw::contact_course`), no `coursein`; back without a clear the player walks
  straight back to the walk's start at 2 a frame (0x7101470e30 → state 10). Enemy points don't
  move. The camera: fixed 460.8 × 259.2 over (168, 110) (0x710170b610), dipping 16 over 15
  frames during a button course-in. The progress record keeps used houses in bits 5–7 (spec §3).
  Captures `superworld-enemy`, `-retreat`, `-pipe`
- ✅ Toad House minigames (`super-worlds.md` §6, read from the three sequences): each house runs
  its kind from the world data over the map — MATCH & WIN (`MyWEdit_Minigame_00`: three strips
  at 45 / 30 / 50 a frame, top and bottom left, centre right; A stops them in turn, four slots on;
  three Stars 5 lives, Flowers 3, Mushrooms 1), POP & WIN (`_01`: A pumps, stages at 4 / 10 / 18
  / 28, bursts at 50 into 1 / 3 / 5 lives, 4 / 4 / 3 in 11), CATCH & WIN (`_02`: ten pitches,
  Fastball / Changeup / Knuckleball / Forkball / Heater on the game's flights, A jumps, the
  game's catch test on the boxes, 3 misses lose, else 5 / 3 / 1 for 0 / 1 / 2 misses), with
  their animations and sounds; one go per house until a game over. Rules `mmk_modes::minigame`,
  scenes `mmk_front::minigame`; captures `superworld-minigame0` / `1` / `2`. ✅ Play Together
  (2026-10-25, branch `swmap3`): Mario, Luigi, Toad, Toadette (`TypeMember`), each with their
  own A (`Host::player_buttons`: the shell and the FFI hand each player's controls over as
  co-op does): any player stops a strip, each pumps with their own animation, each jumps and
  catches with their own box, the Forkball aims at a random player. ⬜ the rumble cues
  (`MW_Minigame_01_Pump`, `_02_Catch`: the hosts have no rumble yet)
- ✅ Enemy-icon courses that start on contact, point icons by model (icon order confirmed,
  0x710143f150) · ⬜ map ghosts · ✅ co-op: the card's
  Play Together (left onto it, or a click; L / R change worlds), every controller plays, one
  shared life counter
- ⬜ Super World maker (later; its rules are written up in `super-worlds.md` §8, 2026-10-25: the
  limits, what makes a world valid — start to castle, the castle's course —, the parts, the
  theme menu's order, the edit states, Worldbot's slots and the one-upload rule; left [?]: what
  makes a world ready for upload)

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
- ✅ The other players' names: each sends its name with the handshake (PROBE / REPLY, the host's
  START carries everyone's; older builds still meet, shown as "Player n"); checked on the
  local server
- ⬜ Difficulty vote and TGRCode courses by difficulty (the owner sends the code); Miis; versus rating; watching mode
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
- ✅ (2026-10-25, `hundred3`) Challenge Modes' 100 Mario on the SMM1 screens runs on SMM1's own
  timing, fades and music and ends in SMM1's rescue scene (Phase 6); new captures
  `challenge-ending-smb3|-smw|-nsmbu`, `challenge-credits`
- ✅ (2026-10-25, `hundred4`) …with SMM1's costume window after the credits, Save & Quit's
  question, the shoe / Yoshi / helmet in the rescue scene and the maps' beat bounces (Phase 6);
  new captures `challenge-costume`, `challenge-costume-last`, `challenge-quit`,
  `challenge-ending-shoe|-yoshi|-helmet`
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
- ✅ Frozen Coins (3.0's `FreezeCoin`, the coin's 0x4 form; found blocking 4 of 38 recent TGRCode
  courses): solid ice round a coin (`<style>_Field_anime_coin_freeze`), the player's fireballs
  thaw it into the coin [inf: which fire thaws; `p_IceCoinThawing` isn't in the imported sound
  users]. 38 of 38 recent TGRCode courses and 550 of the 550 sample fully playable, 0 crashes (2026-10-24; the coverage check reads block, Bill Blaster and Lakitu contents); a second TGRCode sample (new / popular / endless, 2026-10-25): 272 of 273 (left: a Koopaling in a Clown Car — their own Clown context, Move / ShellMove, not traced)
- 🔄 Stacked parts (2026-10-24, from the game's tower manager; research objects/stacks.md): ✅
  play — columns chained as the game does (feet in the row above the lower top, x within 0.2 px
  of the bottom's; Bill Blasters / Cannons as fixed bases; 128 members at most); the base acts
  on its own, riders are carried by 0.9 of the lower one's movement, pulled back (±0.009, decay
  0.005: the column leans), fall onto the lower top (−0.1875, cap 4) and take the base's facing
  4 frames a member up; a lost member's riders fall onto the one below, a flier above it breaks
  away; a rider off the lower body (sideways past it − 5, 64 px up / down) or on the ground
  starts its own column; most riders idle, Piranhas / Bros / Magikoopas … keep acting; drawn
  sway · ⬜ water gravities for riders, the full leave predicate, what each own tower action
  does; in the maker ✅ a carried part dropped on one that stacks goes on top of its column
  (`mmk_editor::stack`) [inf: the gesture] and sounds `Tower_stack` pitched by its row
  (`MusicalScale` = row mod 28, through `Host::edit_sound`; `Combine` / `p_CancelUnion` turned
  out to belong to the parts' 0xC0000 bits, not stacks) · ⬜ the editor's height limit (play
  takes 128 members at most), pulling a member off a column
- 🔄 Physics: re-recording every trace (2026-10-24; the old 428 / 454 was not a true count — the
  old items / form traces were all small Mario). Re-recorded, current engine: **598 / 598 exact**
  over 113 courses + 27 whole-course traces (2026-10-25 physics8: all 9 whole courses exact: W_3064188, W_3046973, W_3065813, W_3006094, W_3049200, and W_3051046, W_3051605, W_3145588, W_3010660 to their deaths, the title course not restarting; and the test courses X_slopejoin, X_slopejoins, X_cpipe, X_cpipeg, X_cpipev ×2, X_cpiped, X_mushstart ×3, X_seesaw, X_seesawp, X_pound ×6) (2026-10-24 physics7: the shell courses now compare the shell too, which showed
  3 carried-shell throws off; and object traces: the player plus every simulated actor matched to
  a recorded game object, found by scans while the game is paused in the trace hook): M1/M3/MW/WU/3W flat 34 each, M1/M3/MW/WU/3W items 34 each, M1/M3/MW/WU/3W
  slope 4 each, M1/M3/MW/WU/3W wall 4 each, M1/M3/MW/WU/3W water 2 each, M3/MW/WU/3W form 16 each
  (WU: the Propeller Mushroom can't be collected at the title screen, so the capture makes the
  Super player a Propeller one, form +0x4a8 = 3),
  M1/M3/MW/WU/3W enemy 2 each (a stomp on a penned Goomba with the held-jump bounce; a running
  jump from against the pen wall onto Munchers (3D World: Piranha Plants): the death, its
  45-frame freeze and the hop), M1/M3/MW/WU koopa 2 each, player and Koopa both compared (a red
  Koopa pacing a 1-wide pillar: the stomp; a kick from above, the shell's slide off the pillar,
  wall bounces back and forth for 280 frames), M1/M3/MW/WU shell 2 each (an empty Buzzy shell:
  walking and running side kicks, the wall bounce, a jump over the returning shell) (was 274 of the first 286 with the engine at
  39fdb6e). Fixed: pit death at view bottom − 56 with no hop, slip end on flat, raccoon tail
  wag, SMW cape take-off / climb end / slide end, jump bonus from the pre-move speed, the wall
  probes at the moved height, solid actors' sides (Munchers) flush like terrain; a foot probe
  exactly on a column boundary stands on the column to its left; an enemy's touch hurts on the
  next frame (after that frame's move; a death then freezes the enemies before they move); 3D
  World's air run target is 3.0 before the dash stage too; red Koopa ledge turns (probe ±3.9
  after the move, the walk back from the next frame); a stomped enemy doesn't move that frame; a
  kick from above doesn't bounce; a kicked shell slides from its next update, stands on its
  centre ±1, meets walls with 6 px sides flush and turns back a frame later; the kicker's
  no-hit window also blocks stomps; NSMBU Koopa contact box 21 px tall; the Propeller launch
  (vy 0.5, +0.7 to 3.0, 38 frames; the fall keeps the rising air control while going up); a side
  kick slides the shell that frame, and a shell slides on past the course's left edge (its
  leftmost column continues); the Cat's air scratch keeps normal gravity and its Down-held pound
  is the normal ground pound (the dive is the Spin press in the air, below); a carried shell
  (SMB3 / SMW / NSMBU) sits 11 px ahead and 1 up (NSMBU 8.909 and 4), stays in the hands 5 frames
  of the Throw state after Run is let go (NSMBU 6), then flies at 2.5 + 0.35·|vx| (≤ 4) with vy
  0.2, moving twice that frame from 3 px ahead; M1 hit (a running kick through two Goombas that
  spawn as the view comes near, the wall bounce, a jump over it; player, shell and both Goombas
  exact): the camera's horizontal follower is 0.3 × distance with no player-speed term (the game's
  view rect found in memory, within 0.0002 px), objects reaching last frame's spawn zone appear and
  act in the same frame, a walker placed on the ground walks from its first frame, a knocked-off
  enemy keeps its speeds on the hit frame and is removed once 40 px below the view, and a shell's
  side hit reaches the block in front of it (it turns a frame later); M1 shellq (the shell into
  a ? Block with a Super Mushroom) and M1 blocks (small Mario's head into a brick, a ? Block twice,
  a ten-coin brick four times, a ? Block's mushroom chased and collected): flush under a block
  (no 0.001 gap), a block's bounce starts the frame after the hit (a shell's side hit lands after
  the blocks' update: one more), its item appears at the block's last offset (+0.429) and doesn't
  move that frame, a mushroom stands on the frame it comes out, stands on its centre ±1 with
  inclusive column edges, is taken in its own update the frame after the touch (one more fall
  step, no landing) and the player grows a frame after that; hit in all five styles (3D World's Kuribo3W walks
  at 0.3 and a knocked-off one is gone after 24 frames), shellbrk (the shell into a Brick Block)
  M1 / M3 / MW / WU, shellq M1 / M3 / MW / WU, blocks M1 4 + M3 3, bigblk M1 3 (Super Mario
  breaking the brick); objects M1: spring (a Trampoline's top 13.5 up, pressed 1 px a frame to 4
  from the first ridden frame, held, the launch 4 frames later after the rider's update: vy 6.0
  held or not, vx half the landing speed, the rider creeping at 0.2 meanwhile; the rise after it
  takes the released gravities up to the first falling frame), note (landing pushes the block
  down from the next frame, the rider goes with it and is launched at 5.2 the frame it comes
  back up, passing through its own cell), donut (per-block 48-frame timers with a 4-frame ridden
  counter, both feet; the drop carries the rider 0.1875 that frame, a higher support wins, the
  rider shows the donut's top below a floor for a frame), lift (an off-track lift paces ±48 at
  min(0.5, d/16 + 0.125), its top 7.95 over its centre; lifts update after the player: feet that
  come over it inside its box are pushed up and carried that frame, a walk-off gets its last
  move a frame late plus a carried 0.5 shrinking by max(10 %, 0.01)), pswitch (stood on, 16 px,
  pressed on the 3rd frame, 4.8 px after), bobomb, note_head; enemies are met where they stood
  before this frame's move; walls are probed under a head stopped by a block. **Whole courses**
  (from the course's first frame, the engine running the game's two pre-script updates): two
  SMB1 courses exact start to goal (W_3064188, W_3046973: player + Goombas; the pole touch is
  taken in the next update after a normal move, the enemies stop, the pole's x from the frame
  after, 12 frames of hold, the slide at −1.9, 93 frames in all, then x + 6 facing right);
  W_3049200 (cloud blocks, Dry Bones, Cheep Cheeps, Koopas) exact to the end (a jump's take-off
  frame from a slope still moves along it; sliding shells are met where they stood before their
  move too; a ground block above a slope's surface holds the feet; at 374 a slope whose low cap
  lies under a ground tile: its first step is full solid (physics8); at 424 a stomp on a falling
  Koopa, measured from its top before its move; its Dry Bones on a cloud row high over a slope exact: a one-way top above a slope's surface holds the feet); W_3065813 exact
  (a wall of dotted-line blocks: their 0x4 variant is the red one, solid at ON), W_3051046 exact to its death at 103 and through it (the title course doesn't restart: compared to the engine's restart), W_3006094 exact (a bridge is stood on at its lower row, the deck under its rails; off-screen actors go against the view after the frame's camera move), W_3051605 exact to the title course's death sequence, the player through its death at 233 (a big Paragoomba walked into while falling past its top: the after-move touch covers winged walkers, with the stomp's own window) to the title course's death sequence (its packed row of Goombas: a walker turns a frame after a wall or an enemy met in front, then 8 frames of Turn, stopped by another enemy met in front; from the binary's Walk / Turn and enemy contact, timing from a recording of the Turn fields; its staircase's six Goombas exact: an actor on an incline steps x · cos θ, lands by its centre, stands a frame after a fall, and walkers turn on each other only head-on), W_3145588 exact to its death at 421 (an unpressed P Switch is solid at its sides; parachute coins are Coin actors, never P Switch bricks; Goomba's Shoe: 50 frames and its wind-up animation before a hop, 24 frames in Walk, 12 winged; a big one's walls 14 px out, flipping off them a frame late; the winged one's hover from the binary, the engine's 256-step sine; it hurts after the player's move like the walkers), W_3010660 exact to its death (the Seesaw as v3.0.3 turns it, physics8: the rider's torque from the third frame its centre is over the plank, the plank tilting after the player's move and carrying the rider by its ends' movement; both Swinging Claws; a mushroom shut in a one-tile box). Object courses in all styles exact
  (3D World's Trampoline eases 3 px, its falling Donut is gone after 21 frames, a lift pushes up
  feet at most 6 px under its top, a foot on a column boundary rides both donuts; SMW turn
  blocks spin 240 frames, passable; small Mario's head at 15.9 in SMW / NSMBU / 3D World; walkers
  hurt in the frame the player's move runs into them; 3D World items touch 13 px apart). The
  **Cat dive** (ClimbBodyAttack) is the Spin (ZL / ZR) press in the air, from the binary: the
  Jump / Fall states' action table puts delegate 0x20 (0x7100a0ce30) in the slot the dispatcher
  runs on input +0x2A (the ZR press) [obs v1.0.0], confirmed by a trace (3W_form form_dive: 4.0 / −3.0
  to the landing, then straight into Walk at the walk speed). 3D World
  slide-end stoop
  3 frames confirmed. 3D World recording works with a local Ryujinx fix (a zero-sized guest
  texture with 2 levels made Metal abort under MoltenVK). ⬜ still to record: the whole courses past their
  first divergences (a Seesaw's tilt, a slope overlapping ground, W_3051605's death at 233)
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
  | NSMBU | 16.1 / 45% | 9.1 / 13% | 15.9 / 31% | 33.4 / 63% | 22.7 / 53% | 7.8 / 14% | 6.7 / 9% | 13.6 / 43% | 35.0 / 62% | 17.6 / 38% | 17.8 |
  | NSMBU night | 14.4 / 24% | 14.3 / 22% | 19.7 / 36% | 15.8 / 47% | 2.8 / 7% | 2.4 / 4% | 13.5 / 22% | 20.9 / 42% | 13.3 / 21% | 29.0 / 40% | 14.6 |

  2026-10-25 (pass 7, branch `pixels7`): SMB1 1.6 / night 1.2, SMB3 2.7 / 1.7, SMW 2.2 / 2.0,
  NSMBU 15.0 / 9.2, 3D World 21.2 (ground 20.7, underground 16.9, castle 22.1, airship 28.5, water
  26.2, ghost 17.4, snow 18.9, desert 13.0, sky 15.1, forest 33.4).

  NSMBU colour combiner (branch tev-wu, 2026-10-24; the uber shader's TEV stages read from the
  game's programs, research/specs/render-3d.md): two registers from `tev_color0` / `tev_color1`,
  stages `saturate((d + mix(a, b, c)) · scale + offset)` over the albedo / `_a1` / `gsys_user0`
  textures (indirect-offset coordinates, each through its own `tex_srt`), constant and vertex
  colours; the lit colour and alpha from `material_diffuse_color` / `material_alpha` → NSMBU day
  39.9 → 27.5, night 39.7 → 21.5 (table above; airship 71.2 → 36.2, sky 68.9 → 38.0, night
  airship 79.9 → 17.5). The castle walls' warm tint dropped (their program lights the plain
  albedo) → 3D World 33.0 → 32.8 (ground 33.3, underground 19.2, castle 31.5, airship 29.6,
  water 35.6, ghost 22.2, snow 74.2, desert 24.6, sky 16.9, forest 41.1). The background model
  placed at SceneDB's `DV_pos_x` / `DV_pos_y` (NSMBU: 1920, 0; vertical areas `DV_V_pos_*`) →
  NSMBU day 27.5 → 18.7 (ground 40.5 → 14.3), night 21.5 → 19.7. The light map as the game's
  light-map program makes it (read from `agl_technique_lght`): per light mix(back-side colour,
  colour, (n·l·½ + ½)^pow) — the LUT domain is the wrapped one —, only the lights the env set's
  `.bagllmap` names (3D World's "Depth" light drops out), with its `effect` and `pow` (Fill and
  hemisphere 50, Main 1); importer 13 / pack 13 → NSMBU day 18.7 → 18.1, night 19.7 → 19.4 (table
  above), 3D World 32.8 → 29.6 (ground 34.9, underground 18.0, castle 32.3, airship 28.3, water
  37.1, ghost 22.4, snow 74.2 → 44.0, desert 19.8, sky 16.9 → 26.4, forest 32.6). 3D World now
  comes out ~25 too bright in the sky and ground themes: a light scale of 0.8 (the uber shader's
  `User0[31].x`, value unknown) measures 28.7 — not adopted without the value. The combiner's
  emission (`_e0` · `emission_mul_color` · `emission_intensity`: 3D World airship's LED blocks)
  → 3D World 29.6 → 29.5 (airship 28.3 → 27.9). `tex_coord0_calc` 6 (the sky theme's field: the
  world projection pushed along the normal) world-projected too → sky 26.4 → 26.0. Tried and
  dropped: the env sets' colour-correction saturation (`.baglccr`, 1.125–1.2) → 3D World 30.5–31.0.
  The edge (rim) light (`enable_edge_light`: (1 − n·v)^(sharpness·10) · intensity · colour ·
  mix(1, L(−v), rim_i), from the 3W castle field's program) → NSMBU day 18.1 → 17.8 (table
  above), night 19.4, 3D World 29.5 → 29.4 (ground 34.4, underground 18.0, castle 32.1, airship
  27.9, water 38.3, ghost 22.4, snow 43.3, desert 19.4, sky 26.0, forest 32.6).
  Every texture with its own sampler's wrap modes (imported per shader sampler, mesh `wraps`;
  importer 13: the night sky's noise and sky layers repeat where the albedo clamps) → NSMBU night
  19.4 → 14.6 (table above), day 17.8, 3D World 29.5 (ground 34.4 → 34.7).
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
  3D World's dash footsteps ✅; found by rendering the 550-course sample's sound logs
  (`mmk audio-render --log`, run and walk) against the data: 3D World's enemy fireball spit on
  the Fire Piranha Plant (land / water), helmets on / off, a shoe knocked off, a key door that
  won't open (`key_notget`), breaking an action condition (`p_lose_missionClear`), 3D World's
  donut blocks (shake, drop, back), blinking blocks and timed spike blocks, the conveyor loop
  (on screen; fast belts' own) ✅ · ⬜ the donut shake as a loop, 3D World flip panels, ice / cloud block breaks, the
  power-ups' item-specific sounds the scripted runs never reach

**G9 — Every platform** 🔄
- 🔄 macOS (Swift app) · 🔄 iOS / iPadOS (built for the Simulator; the owner's iPad) · 🔄 Windows ·
  🔄 Linux · 🔄 web · 🔄 Android
- 🔄 Android (`crates/android`, `scripts/build-android.sh`): the portable shell and the SMM2 front end in
  a NativeActivity (no Java code, no Gradle), arm64, API 28+, Vulkan. On-screen controls (d-pad,
  Jump / Run / Spin, +; several fingers, drawn while playing; a touch elsewhere is a tap for the
  menus), the surface dropped and remade across background / foreground. Checked in the Android
  Emulator (API 35, arm64) with an asset pack pushed by `build-android.sh --push`: opening, title,
  Play menu, Course World, the library list, a course page and play by touch. Open: the import on
  the device (G1's NSP importer + a file picker), a real phone, Bluetooth controllers' buttons and D-pad as the keyboard's keys ✅ (`input::android_pad_key`;
  compiles for the target, not tried on a device; the sticks need motion events winit doesn't pass), the 3D World sky drawn in stripes in the emulator, sound not checked (emulator audio off)
- ⏳ Release packages for every platform (the game only; the player imports their own dump):
  `release.yml` builds Windows, Linux x64 / arm64, macOS, Android and the web on a `v*` tag;
  waiting for the owner's OK to tag the first one (CHANGELOG "Unreleased" is ready)

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
  ending text, credits, costume reward); ✅ (2026-10-25, branch `hundred3`) the run-flow timing
  between screens (smm1-100mario-screens.md §8: the map class's states, SMM1's scene fader types /
  speeds, the input locks), the rescue scene (smm1-ending.md §2.8: its models decoded), the Mii /
  flag / Star / comment areas (§8b), the SMM1 → SMM2 course import design (smm1-course-import.md)
- 🔄 SMM1 asset import (owner's disc dump): BFLYT/BFLAN layouts ✅, GX2 textures (BFLIM, AddrLib
  1D/2D untiling) ✅, BFFNT fonts ✅, MSBT text (English) ✅, UI sounds ✅ (2026-10-25: the banks of
  `Pack/Sound.pack` — SARCs of AMTA + BFWAV, not BARS — and the UI streams (BFSTM) → `ui/sound/*.wav`,
  417 sounds; the SLink2DB v0x15 (`mmk_formats::xlink_wiiu`) → `ui/sound/sounds.json`, 968 users;
  `mmk audio` reads SMM1 waves, banks, `Sound.pack` and streams; importer 4), BGM streams ✅
  (2026-10-25, importer 5: Course World's tracks 2 / 3 / 6, the maps', game over's, the rescue
  scene's by style, the staff rolls; loops in a `smpl` chunk) · the rescue scene's models ✅
  (importer 5: `ui/ending/ending.json`, `mmk_formats::bfres_wiiu::{models, tex_patterns}`)
- 🔄 Layout player (panes, text, keyframed animations) — pane tree, parts, pictures, windows,
  text with font fallback, SRT/visibility/alpha/colour/texture-pattern/texture-SRT curves ✅;
  part property overrides: picture material from the enclosing layout (course-dot numbers,
  castle variants) and basic values (visibility, translate, size, scale, rotate, alpha; import
  now keeps them) ✅; "influenced alpha" fades the whole subtree incl. parts ✅; children without
  anchor bits sit on the parent's origin point [inf] ✅; caller visibility, pane query, message
  templates with value placeholders (FFI `mmk_ui_set_visible/pane/message`) ✅;
  per-character transforms ✅ (2026-10-25: `Text::per_char`, FLCC / FLCT; GAME OVER's letters),
  alpha compare, indirect textures ⬜
- ✅ Mode logic (`core/crates/modes`): run lengths 8/16/16/6 (10 Mario 8), 100 lives = cap, ≤3
  banked extra lives paid on clear (+1 refund in 100 Mario), death retries from the course-start
  state, free skip (100 Mario only), Save & Quit, game over, completion; course pools by
  difficulty band with SMM1's shuffle and castle search; Continue records, unlock rules, stats ·
  costume reward selection ✅ (2026-10-24: the manifest's `rare` categories, `hundred::costume_reward`)
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
  - ✅ UI sounds (2026-10-25, `mmk_front::smm1_sound`): the SLink users `UI_<layout>[_<part>]`'s
    keys and states where the screens play their animations — the tabs' curtain, the start
    buttons, the map demo (Telop `M3_StarsGather`, the Goombas' footsteps, Peach's "Help!", Mario's
    jump / walk, the wipe, democancel on a skip), the card's 100 Marios (walk sound by the lives),
    pause In / Out by the course style, the buttons' cursor and decide sounds, course clear, the
    skip countdown and SlidePass; key ↔ animation [inf] where the data names only the key
  - ✅ (2026-10-25, branch `hundred3`) timings from Block.rpx v208: the map demos state by state
    (New Game 1128 frames on Easy, Continue, after a clear), SMM1's scene fader (`smm1_fade`: type 1
    at 4.0 for every scene request, blocks at 1.0 to the map, the circle on Mario at 2.0 from it),
    the course card over the course for every course (the course waits), a death / Start Over
    fading to the card, the input locks (select 48 / tab slides 64, clear buttons after In 180,
    skip after 112), no early exits; "clear shown" kept as `hundred.clear_shown`
  - ✅ the rescue scene (`smm1_ending`): SMM1's stage, Peach / Toad and the demo Mario in his goal
    form (SMB1 / SMB3 / SMW; NSMBU and 3D World use SMW's) on the scene's own timers and camera,
    the balloon's line by difficulty and costume, then the credits with no input; the menus' music
    (`CueKind::Music` / `MusicTrack` / `StopMusic`, shell mixer + Swift); GAME OVER letter by letter
  - ✅ the maker group centred as SMM1 does, the pause's stars and players from the course data;
    the flag (no country), the Star button and comments (no service) stay out [departure]
  - ✅ (2026-10-25, branch `hundred4`) the costume window after the credits: SMM1's Mystery
    Mushroom list in its get form (`smm1_costume`: the costume's title and name over its column,
    the list scrolled to it with the owned costumes' art, its Valid reveal and trumpet, the A icon
    after 180 frames, A or a tap closes it), then the "every costume" message (`smm1_window`,
    `Cmn_CmnWindow_00`, Msg_0051 / 0052); Save & Quit's question in the same window (Msg_0006, No /
    Yes) [obs 0x0259f4c4, 0x023ea714, 0x0241c510, 0x02413274]
  - ✅ (`hundred4`) the demo Mario's Kuribo's Shoe (hops on 0x025eed54's constants, the shoe poses),
    Yoshi in SMW's scene (his skeletal run / wait baked per frame, Mario on the saddle bone) and
    the shell helmets; importer 6 (`ui/ending/ending.json` v2, `bfres_wiiu::skeletal_anims`)
  - ✅ (`hundred4`) the maps' beat bounces on the music (`smm1_beat`: the tunes' AMTA markers in
    `ui/sound/bgm.json`, half-beats, BeatOffset + 31, the castles' Bowser on `Koopa`); the
    between-course maps' tunes have no beats [obs 0x02089898]; the card has no beat animations
  - ✅ (`hundred4`) hundred3's guesses: the projection 384 × 216 [obs 0x0273e5dc], black behind the
    SMB3 / SMW stages [obs 0x02703e14], per-character time `i · width / (n − 1)` [obs 0x02892384],
    the rotation pivot (centre / bottom, obs 0x02885dc8), after a skip CourseInScene through the
    plain fade [obs 0x02379580], the MapIn skip flag set by the same A [obs 0x0258ea40], the cover
    held until the new scene has loaded [obs 0x02089750] · still [inf]: the music at a scene
    change (the assets' StopFrame 0; the controller's 2.0 default), the half-beat mode
  - ⬜ left: NSMBU's 3D demo Mario (still SMW's scene; plan in DECISIONS 2026-10-25), the WU
    perspective camera (camera object 8)
- ✅ SMM1 → SMM2 course import: design and the binary's answers (private
  research/specs/smm1-course-import.md, 2026-10-25: the width in pixels and the goal from it,
  pipes' 0x80 = "holds a part", tracks = SMM2's end words, the effects by their waves);
  converter ✅ (`hundred4`: `mmk import-smm1-course`, `mmk_import::smm1_course`; `<stem>.course`
  + `<stem>.losses.json`): the 76 built-ins all convert, load and simulate (0 crashes, 6 cleared by
  the scripted run), **76 / 76 fully playable** (`smm1lib`, 2026-10-25: Bowser's 0x4 is SMM1's
  Bowser Jr. → part 98 [obs v208 resolver 0x020ac8ac]; Donut Blocks on tracks / with wings
  [obs v3.0.3 0x7101389df0 …]); losses: 23 effects (samba / disco, bird whistle, telephone), 20
  Mystery Mushrooms (as Super Mushrooms), 30 start areas widened · ✅ the SMM1 import (importer 7)
  converts the 60 sample courses into `<smm1 import>/courses/` with their names from the game's
  text; the library lists them as built-in courses, collection "SMM1 Sample" (DECISIONS
  2026-10-25) · ⬜ a front-end place for them (SMM1's Coursebot Sample Courses tab / the 10
  Mario Challenge pool)
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
    request; opening, title over its demo courses, Play menu, Story Mode, 100 Mario, course card /
    HUD / pause / give up; UI sounds, course music, keys, mouse clicks; saves in `localStorage`
  - ✅ (2026-10-25) Course World on TGRCode from the page (CORS `*`; the native client, its
    requests fetched by the page, thumbnails last, 429s waited out): lists, thumbnails, course
    page, Play, Download (kept in OPFS `mmk-save/`); the library stays a Mods setting
  - ✅ (2026-10-25) the Course Maker: still view, part icons, test play, maker music and edit
    sounds, the Coursebot (slots kept in OPFS `mmk-save/`, still there after a reload); the
    maker screen is pixel-identical to the native shell's capture (WebGPU, linear layout colours)
  - ✅ (2026-10-25) the engine's data that it read from disk: tables (backdrops were black on every
    course), 3D environments, particle effects, NSMBU / 3D World models (read as asked for; 3D
    World was drawn with 2D sprites); the page now copies `meshes/` and `anims/` too
  - ✅ (2026-10-25, branch `webparity`; checked in headless Edge: smoke.mjs --server --coop --lost all PASS) the own
    course server from the page (`web_server`: `mmk_online::Client` over a `Transport` the page
    fetches for; server CORS for Private Network Access): Course World's list, thumbnails, Play,
    Download, account register / token sign-in (token in `localStorage`, paste into the field),
    likes / comments / leaderboards, replays and ghosts, the Coursebot's upload (any maker name:
    `?maker=`); page "Course server…" / `?server=`
  - ✅ (2026-10-25, `webparity`; the --pool smoke step not run yet) 100 Mario / Endless from Course World's pool (`web_pool`):
    TGRCode's Endless draws in the background (behind the player's own requests), kept in OPFS
    `mmk-save/pool/` (100 at most), or the own server's `/api/pool`
  - ✅ (2026-10-25, `webparity`, browser-checked) local co-op: one player per Gamepad API pad as natively
    (`Host::controllers` / `set_players`), or the keyboard split in two (page "Keyboard: 2
    players" / `?split=1`; natively `MMK_KEYBOARD_SPLIT=1`)
  - ✅ (2026-10-25, `webparity`, browser-checked) a lost graphics device: made again on the same canvas up to 3
    times (UI layers and renderer rebuilt from the caches), else a message in the status line;
    WebGPU errors are logged instead of panicking
  - 🔄 (2026-10-25, `webparity`) memory: the models' file bytes are dropped once parsed past
    64 MB (`HandedFiles::with_cap`, least recently taken first); parsed meshes and texture
    pixels still stay for the visit
  - checked 2026-10-25 by `research/tools/web-check/smoke.mjs` in headless Edge 156 (WebGPU on
    Metal; Playwright's chrome-headless-shell loses its WebGPU device on any canvas): load 1.3 s,
    title / maker / 3D World course at 60 fps (p95 16.8 ms), keys, clicks, first-key sound,
    Coursebot persistence, TGRCode; `import-check.mjs`: the NSP import. Earlier: headless Firefox
    (WebDriver BiDi; before the menus), a scripted Gamepad API pad, non-silent WebAudio. Open:
    Safari (OPFS `createWritable`), Firefox with the menus, a real controller in a browser, touch
    controls, mobile memory limits (parsed models and texture pixels are kept for the visit), a
    browser check of the `webparity` items above
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
  it too (UI cache over an in-memory file source filled from OPFS, textures on request; Course
  World on TGRCode and the Course Maker since 2026-10-25; the course server's HTTP API from the
  page on branch `webparity`). Open: Settings screens, ghosts in the Apple app

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
      Ninji / Super Worlds over the course server, layout scissor clipping, the pause blur / clear capture effects, menus in `mmk-play`
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
- ✅ Clear Pipes play (2026-10-25, branch `clearpipe2`; research objects/93-clear-pipe.md, all v3.0.3):
  the walls (every pipe tile solid), the game's point paths (0x7100e5a230), the traveller (3.0 px/f
  for everyone, 10°/f bends of radius 16, 8 px past the far mouth, pipes joined end to end ridden as
  one, a blocked exit pauses 5 frames and turns back: 0x7100d86d90 / 0x7100d87a50), the player's
  DrawDokanMove (entries: the direction held 10 frames or pushing a side mouth, Down on an upward
  mouth, Up under a downward one, 7.89 px; rolled ±90° on vertical legs, set down by half the body out
  of a side mouth, stoops if there is no room; 0x71015aa8c0, 0x71015afe50 / 0x71015b0300 /
  0x71015b0d40), enemies' and items' DrawDokan (walked into a side mouth, 16 px; the EnemyDB rows'
  offset y / rotate type / out type; out at 4 px/f easing to 0.5, 10 frames, re-entry 16 px ahead;
  0x71012100e0, cDrawDokan 0x71028b3d20), the sounds `route_dokan_in` / `_move` / `_out`, riders
  drawn in front, rolled (Sprite.roll) or at 66 % (items); the blocked tests (the player's: room
  for the body, 0x71015acf60; enemies': 14 px rays, a mouth just ahead is no block); the pipes
  render again in the maker and play (their materials' colour combiner came out fully transparent:
  a pale glass stand-in for the game's own clear-pipe shader, host `clear_pipe_glass` [inf]) ·
  ⬜ the block rays' terrain filters, the BgUnit's own mouth faces (full tiles here), the game's
  clear-pipe shader (fresnel / refraction), the in-pipe player scale table, carrying / Yoshi / boxes through pipes, multiplayer screen
  edges, the Screw spin about the vertical (not drawn); captures: `research/specs/objects/
  93-clear-pipe.md` "How to verify"
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
