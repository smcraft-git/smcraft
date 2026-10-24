# Changelog

Each release's packages hold the game only: no Nintendo data. You import your own copy of
Super Mario Maker 2 once (the NSP, the 3.0.3 update's NSP and your `prod.keys`) and after that
SMCraft never needs them again.

## Unreleased

SMCraft is back in development (it was archived in October 2026).

- **Clear Pipes play as in SMM2** (3D World): their walls are solid, and Mario walks into a side
  mouth (holding the direction, or pushing it), drops into an upward one with Down or jumps into a
  downward one holding Up; he's carried through at the game's speed, round bends and through pipes
  joined end to end, and comes out the far end — or turns back when the way out is blocked. Enemies
  and items that walk into a mouth ride through too and shoot out of the other end. Riders are
  drawn in front of the pipe, turned along it (items shrunk), with the pipe's in / along / out
  sounds.
- **Wings as in SMM2**: winged power-ups, coins, springs, P Switches and POW Blocks fly slowly
  the way they face with a 16-px bob and pass through walls (they meet walls only in 3D World);
  winged blocks fly the same way, and winged Hard, Ice and Donut Blocks swing 48 px from side to
  side instead; a winged Spiny sets off left and hovers in a slow bob; a winged lift keeps pacing
  with an 8-px bob, and a winged falling lift bobs and falls slowly; a Lakitu's coins fly on after
  they are thrown.
- A lift that's set to start right now starts right; the Angry Sun waits 5 seconds before its
  first swoop; the cape spin lasts 20 frames; a Side Thwomp notices big Mario by his head; a Chain
  Chomp flies back onto its stake along the game's curve; a Red Yoshi's Egg is the one with the
  big flag; a custom autoscroll node set to "stop" stops the camera, and vertical custom
  autoscroll uses the game's vertical speeds.
- **Swinging Claws swing as in SMM2**: the claw hangs on its rope 48 px under the pivot and an
  empty one starts at rest, swaying by itself in a slowly growing swing (it was a wide 60°
  pendulum); a caught player's speed carries into the swing and left / right nudge it. Claws are
  set up as the course starts, like the game's.
- **Twisters carry Mario as in SMM2**: a Twister's column lifts Mario off the ground and up at a
  steady speed (it was a guess), first pulling him toward its middle for half a second; near the
  top he bobs; a jump inside it sends him up faster; at night in the Sky theme it lifts more
  gently. The column widens toward its top.
- Stacked parts: a column comes out together when the whole column reaches the screen (as the
  game's tower record does); a stomped Koopa or Buzzy Beetle in a column drops out of it as its
  shell; a rider pushed against a wall is no longer stopped dead but keeps riding along.
- More winged enemies as in SMM2: a winged Bob-omb flies level, bobbing 8 px up and down and
  turning at walls; a winged Dry Bones flies like a red Paratroopa; winged Buzzy Beetles and
  Spike Tops fly like a winged Spiny but don't turn at walls; winged Boos, Cheep Cheeps,
  Fish Bones, Wigglers, Magikoopas, Hammer Bros, Monty Moles, Bloopers and Chain Chomps act as
  their wingless selves (the game runs the same behaviour for both) instead of drifting.
- Bowser chasing you jumps walls in his way (a big jump for a tall one) and jumps across gaps,
  aiming to land on the far side, instead of only turning round at ledges. A Yoshi's Egg hatches
  40 frames after it first lands on screen (it was 30 frames after appearing).

- **Import once, then no dump or keys.** SMCraft reads your NSPs and `prod.keys` itself (no
  hactool, nothing extracted) and writes its own asset pack; the game runs from the pack alone.
  On macOS the setup screen picks the files; on Windows, Linux and Android you put them in the
  data folder's `import/` folder and the first launch imports them; on iOS / iPadOS you pick them
  in Files. `mmk import --nsp <base> --update <update> --keys <prod.keys>` does the same from the
  command line.
- **Course World over TGRCode**: the course lists, course and maker pages with thumbnails,
  comments, search by course or maker ID, Play and Download. Endless and 100 Mario draw from it
  too. Your own course server stays a choice.
- **The web version, checked in a real browser**: importing your NSPs and `prod.keys` in the page
  now keeps every texture (it kept only a few hundred), courses show their backgrounds (they were
  black), 3D World draws its models, Course World lists TGRCode's courses from the browser, and
  the Course Maker works (part icons, test play, its music and sounds, the Coursebot, whose
  courses stay in the browser between visits).
- **The web version catches up with the desktop**: your own course server from the browser
  (Course World's lists, search, Play and Download, an account with its token kept in the
  browser, and the Coursebot's uploads); 100 Mario and Endless draw from TGRCode's courses (or
  your server's) instead of only the built-in ones; local co-op with several gamepads, or two
  players on one keyboard; a lost graphics device is restarted instead of ending the game; the
  3D models' files no longer all stay in memory once read (better on phones).
- **Uploads under any name**: a maker name in Japanese (or anything beyond plain ASCII) no
  longer stops an upload to the course server.
- **Android**: SMCraft runs as an APK (arm64, Android 9+, Vulkan), with on-screen controls.
- **Mods from the SMM2 Recomp**: SMM1's Muncher hit box, SMM1's overlapping pipes in the Course
  Maker, quick level return after a miss, and SMW's goal wipe in Super Mario World courses.
- **Network Play groundwork**: signing in to Nextendo Network and matchmaking SMCraft players
  (tested against a local server; live play waits for Nextendo).
- **SMM1's sounds on the SMM1-look 100 Mario screens**: the difficulty tabs' curtain, the start
  buttons, the world map's demo (the banner, the Goombas' footsteps, Peach's "Help!", Mario's jump
  and walk, the wipe), the 100 Marios on the course card, the pause menu's style sound, course
  clear and the skip countdown, all from your SMM1 dump's own sound data (import SMM1 again:
  `mmk import-smm1`).
- **Koopalings in Koopa Clown Cars**: the car chases you while the Koopaling fires volleys of
  his magic (Lemmy his balls, Wendy her rings) from it; a stomp stops the car while he ducks in,
  then he spins in his shell for a while (stomps bounce off) before he comes out; the third stomp
  knocks him out of the car (NSMBU: after his flinch) and the empty car can be ridden.
- Fix: a Koopa Clown Car whose rider is defeated floats empty and can be ridden.
- **Fire Koopa Clown Cars shoot with an enemy inside**: every three to four and a half seconds the
  car slows down, charges and fires a fireball that flies straight ahead (faster when the car is
  moving that way) until it hits a wall; Bowser, Bowser Jr. and Boom Boom in a Fire car too.
  Flying one yourself, it fires straight fireballs each time you press or let go of Run (a
  quick cooldown; three at once as Fire Mario); before, its fireball vanished at once.
- **Parachutes as in SMM2 for Thwomps, Lava Bubbles, Cheep Cheeps and Porcupuffers**: a Thwomp
  keeps its parachute folded and opens it each time it drops (then floats down slowly); a Lava
  Bubble's parachute ends in the lava and it leaps from there; a Cheep Cheep or Porcupuffer lets go
  of its parachute as soon as it is in water.
- **Ceiling Spinies and Buzzy Beetles**: placed upside down under a ceiling, or coming out of a
  downward pipe, they walk along the ceiling, turn at its edges and drop in their shells when you
  pass underneath (a Spiny's shell keeps its spikes and it walks out again later).
- **Boo Buddies in a block** come out as a circle of Boos spreading round the block; a part in a
  block keeps its other form (a lit Bob-omb, a Sledge Bro, a fast Spike Top …).
- Fix: a note block bounced out of view keeps what it holds for a later bounce, as in SMM2.
- **Enemies out of blocks and Bill Blasters under water** behave as in SMM2: a Goomba stays put a
  moment and then swims, a Lava Bubble leaps lower, a Fish Bone glides straight and slows to a
  stop, a Wiggler swims off at once. A winged red Koopa out of a block now slows its climb as in
  SMM2 (it used to brake as it does under water).
- **The P-Balloon and the 3D World wearables out of a block** glide up to the block top as in
  SMM2 (slower under water) instead of rising like a mushroom, and can't be taken until they
  are out; a winged one hops. **A winged or parachuted coin in a block** pops up and floats back
  a little before it flies (or drifts down), and can only be collected once clear of the block.
  A **winged coin out of a side pipe** flies straight out as from a Bill Blaster.
- **Wigglers and Spinies out of a block under water**: a Wiggler can be touched at once and
  drifts out until it slows; a Spiny drops out curled up and stops sooner. On land, both hand
  over as soon as they reach water.
- **Side Thwomps with a parachute** wait in place like Thwomps; when one charges, the parachute
  opens and it carries on slowly. The **Fire Koopa Clown Car's charged shot** (hold Run a second
  and let go) flies faster than its fireballs and keeps going until it hits a wall.
- Fix: Munchers hurt on a wall sensor's touch, as in SMM2 (a one-tile gap between Munchers is as
  tight as in the game).
- **The Course Maker**: SMM2's own maker screens (part palette and part window, styles, themes,
  timer, autoscroll, clear conditions, multigrab, stretching, test play, the Undodog and the
  rocket) with its music and the parts' sounds, and the **Coursebot** (120 slots, Save As,
  Load, Upload). Wings and parachutes from a part's attribute balloon (hold a part still).
  **Mario's Trail**: after a test play the maker shows where Mario went, faint pictures of him
  along the way as in SMM2; the left column's button switches it on and off. **Water Level**
  in the castle and forest: drag the water, lava or poison line's button to make it rise or fall,
  tap it for rising and falling, hold it for the speed. **Custom Scroll**: choose Custom in the
  Autoscroll window, then lay the camera's path (drag the path to add a point, tap a point's
  square for its speed). Both modes draw SMM2's own lines, handles, birds and path marks, and
  sound as in SMM2. Any course can be opened in the maker: someone else's course saves and uploads
  as a new course, "based on <code> by <maker>".
- **Course Maker: the Clear Pipe** (3D World): press and drag to draw it, round bends to the
  side, back to shorten it, from either mouth to draw it on; carry it by its middle, erase it
  whole; drawn with 3D World's own glass pipe. **Every form in the part balloon**: a Fast
  Conveyor, a 30- or 50-Coin, a P or Key Door, pipe colours, semisolid looks (a tap steps through
  them; a fast conveyor made in the maker now plays fast). **Tracks** draw as in SMM2: nothing on a
  press, a piece per step as you drag, on from an end (open ends stay open), back to take pieces
  away. A door or warp box brings its partner a tile to the right. Riders and parachutes stay out
  of stacks.
- **SMM2's colours on its screens**: buttons, banners and text now show the colours SMM2 shows
  (the yellow OK and menu buttons were orange), and windows with flipped frames are whole again
  (the tips' window lost its bottom edge). Import the SMM2 UI again for the maker's new pictures
  (SMCraft asks for it).
- **Super Worlds**: play a Super World (its map, lives, progress), made from your library or
  downloaded from TGRCode.
- **Super World maps drawn like the game's**: each world's theme backdrop and land (water,
  clouds, lava, forest, desert…), the roads with bridges, water routes and ladders, the course
  points by their icon (ghost houses, towers, airships, enemies…) turning blue when cleared,
  castles, pipes, Toad Houses and the player walking, swimming or climbing the map. Needs the
  UI pack imported again.
- **Super World maps with their music and their moments**: each theme's map music; a cleared
  course's point turns cleared with its sound, the player jumps, and the roads it opened appear
  (roads and the goal stay hidden until reached; points become reachable with a pop); the
  castle rises when its road opens; a used Toad House closes; a cleared world's castle crumbles
  and a cannon shoots the player to the next world, where he lands. Ledges stand on cliffs. The
  goal is the airship castle when it stands on water. Needs the game pack imported again
  (pack 14).
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
- **The Cursed Key** (SMB1): Phanto swoops after whoever holds it, and drifts off when they don't.
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
- **Skies and airships that move as in SMM2**: the day skies' clouds and the airships' skies
  drift at the game's own speeds, NSMBU's clouds and 3D World's skies scroll, the airship
  starts on a cloud, and animated blocks and coins keep the game's timing. **Re-import the game
  pack** (pack 15).
- Release packages for Windows, Linux (x64, arm64), macOS, Android and the web.
