# APPENDIX [A-3]: DOOM.WAD changes and errors

  There are some imperfections in the `DOOM.WAD` file. All versions up
to 1.666 have the `SW18_7` lump included twice. Versions before 1.666
have the `COMP03_8` lump twice. And with version 1.666 somebody really
messed up, because every single `DP*` and `DS*` and `D_*` lump that's in
the shareware `DOOM1.WAD` is in the registered `DOOM.WAD` twice. The error
doesn't adversely affect play in any way, but it does take up an
unnecessary 800k on the hard drive.
  
  Some of the lumps in the sprite section are unused. Versions before
1.666 had `PBULx0` and `PSHEx0`, `x=A-B`, which were pictures of bullet and
shell casings being ejected after the player fired a weapon (this
feature was obviously removed). Also there were four more "fireball"
sprite-lump sets: `BAL3x0`, `BAL4x0`, `x=A-E`, and `BAL5x0` and `BAL6x0`, `x=A-B`.
The only unused lump left in 1.666 is `SMT2A0`, which is a small grey
stalagmite, similar to the `SMIT` sprite which is `THING #47`. There are
some new sprite lumps in version 1.666 of DOOM 1 which are "semi-unused"
because they aren't used in DOOM 1, but they ARE used in DOOM 2. They
are all projectile sprites, so it's probably just a little leftover
from compiling the WAD.
  
  So, in case it might help with converting demos, or for any other
reason, here is a list, entirely by Paul Falstad, of all the changes
to the levels between DOOM 1.2 and DOOM 1.666. The 1.4 and 1.5 beta's
levels are in most if not all respects identical to 1.666's - I haven't
checked.

E1M2
- Linedef #530 changed from a door that closes to one that stays open.
  This is the south side of the door out of the maze.  This allows
  deathmatch players who started in there to get out from the inside.

E1M4
- The swastika got changed to a different shape.  A bunch of things in
  the swastika room got moved around to accomodate the new layout.
- Thing #185 (a deathmatch start position) got moved from (768, 1952) to
  (736, 1824).  This is in the room with the ledge NW of the player 1
  starting room; the deathmatch start got moved off the ledge onto the
  main floor.

E1M5
- Thing #216 (a deathmatch start) got moved from (-2112, 512) to 
  (-800, 1200); that is, it got moved from the west courtyard (the one
  with the supercharger) to the hidden hallway just south of the pentagram.
- Sector #105's floor was lowered from 88 to 80.  In other words, the
  window west of the yellow keycard was enlarged a bit.  Also, the
  associated linedefs are no longer marked impassable.

E1M6
- Thing #116 (the sargeant in the middle of void space in the southeast
  corner of the map) got removed.
- Sectors #139 and #142 got their floor changed from `FLOOR0_6` to `FLOOR4_8`
  for consistency with the surrounding sectors.  (These are the floors
  underneath the yellow doors on the northwest and northeast corners
  of one of the rooms.)

E1M7
- Linedef #782 was type 0; now it's type 31 (door that stays open).
  This is south side of the last door before the exit door; it can now
  be opened from the inside, so a deathmatch player that started in the
  exit room can get out.

E1M8
- The computer map in the pentagram got changed to a shotgun.
- Linedefs 35, 136, and 140 no longer have their upper textures unpegged.
  This is the secret door to the supercharger; it now looks like a real
  door when it opens.
- A secret door was added in the east baron's alcove. When you push on
  the east wall, a secret chamber opens with a switch.  That switch
  lowers the lift to the south, so that you can get back into the complex.
  (Though you could anyway, by jumping through the window on the west or
  east side of the hallway south of the lift...)
  Actually, it lowers the lift to the lowest adjacent floor, which
  (after the two barons are dead) is lower than the hallway floor
  height.  Probably not the intended effect.
- Vertex #223 got moved ever so slightly NW for some reason.

E2M4
- Northwest of the big green "O", there is a secret room with partial
  invisibility.  The door to the room closes when you walk north through
  a hallway just southwest of it; you're supposed to shoot the door to
  open it.  However, if you run north quickly over the trigger line and
  then run east through the door, you can just make it before the door
  closes, but in 1.2 you'd be trapped inside, since the door would not
  open from the east side.  In 1.666, the linedef type of the east edge
  of the door has been changed so that you can open the door from inside
  the secret room.

E3M1
- Sector 8's trigger number is now 0.  Previously, it was 6, which is the
  same number as one of the lines you walk over when getting the shotgun.
  This line would cause the floor to be lowered.  However, sector 8's floor
  is already lower than that of any adjacent sectors, so nothing happened.

E3M4
- Sidedefs 1327 and 1332 had their texture offsets fixed.  These are
  the sidedefs on either side of the window between the room with the
  beserker and the room with two spectres and a teleporter, east of
  the player one starting point.  Now, the window looks better than it
  did before, but still not perfect.

E3M6
- There is now a BFG9000 sitting in the northwest window in the building
  which you're facing at the start of the level.  It only appears in
  multiplayer mode.
- The structure which has the switch leading to the secret level had its
  north wall thickened, so that you can't trigger the switch from outside
  of the structure.
