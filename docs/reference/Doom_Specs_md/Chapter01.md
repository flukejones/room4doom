# CHAPTER [1]: Author's Notes

## [1-1]: id Software's Copyright and the Shareware Version

The LICENSE.DOC says:

'You may not: rent, lease, modify, translate, disassemble, decompile,
reverse engineer, or create derivative works based upon the Software.
Notwithstanding the foregoing, you may create a map editor, modify
maps and make your own maps (collectively referenced as the "Permitted
Derivative Works") for the Software. You may not sell or distribute
any Permitted Derivative Works but you may exchange the Permitted
Derivative Works at no charge amongst other end-users.'<br />
'(except for backup purposes) You may not otherwise reproduce, copy or
disclose to others, in whole or in any part, the Software.'

I think it is clear that you may not distribute a wad file that contains any of the original data resources from DOOM.WAD. A level that only has new things should be distributed as a pwad with only two entries in its directory (explained below, in chapter [2]) - e.g. E3M1 and THINGS. And the THINGS resource in the pwad should be substantially different from the original one in DOOM.WAD. You should not distribute any pwad files that contain episode one maps. Here's an excerpt from README.EXE:

'id Software respectfully requests that you do not modify the levels
for the shareware version of DOOM. We feel that the distribution of
new levels that work with the shareware version of DOOM will lessen a
potential user's incentive to purchase the registered version.
'If you would like to work with modified levels of DOOM, we encourage
you to purchase the registered version of the game.'

Recently, Jay Wilbur of id Software announced the formulation of a policy on third-party additions to the game. You can find the announcement on alt.games.doom, and probably lots of other places too. Or you can send me mail asking for a copy of the announcement. Basically, they are preparing a document, and if it was done, then I could tell you more, but it isn't finished at the time I'm writing this.

If you're making add-ons, plan on them not working on the shareware game, and plan on including statements about the trademarks and copyrights that id Software owns, as well as disclaimers that they won't support your add-on product, nor will they support DOOM after it has been modified.

## [1-2]: What's New in the 1.3 Specs
The main reason for this release of the specs, 1.3, is of course the explanation of the NODES structure. I've been delaying a little bit, because I wanted to see if it would be feasible to include a good algorithm herein. Also, I wanted to wait and see if someone could actually implement "node theory" in a level editor, thereby verifying it.

Now the theory HAS been verified. However, the actual implementation is still being worked on (debugged) as I'm writing this. Also, I don't want to steal anyone's hard work outright. This means that there is NOT a node creation algorithm here, but I do outline how one can be done. I have tried to come up with one on my own, but it is too difficult for me, especially with all the other things I'm simultaneously doing.

Where you WILL find pseudo-code is in the BLOCKMAP section. I borrowed an excellent idea from a contributor, and code based on the algorithm given here should be very fast. Even huge levels should recalculate in seconds.

Another new section completely explains the REJECT resource.

This entire document has been re-formatted, and there have been several other additions, and hopefully the last of the typos has been rooted out. I consider these specs to be at least 95% complete. There are only minor gaps in the information now. If the promised "official specifications" were released today, I expect this would compare favorably with them (although I know exactly what parts of it I would look to first).

I've been notified of something very disappointing, and after a couple weeks of trying there seems to be no way around it. The pictures that are used for sprites (things like barrels, demons, and the player's pistol) all have to be listed together in one .WAD file. This means that they don't work from pwad files. The same thing goes for the floor pictures. Luckily, the walls are done in a more flexible way, so they work in pwads. All this is explained in chapter [5].

## [1-3]: Acknowledgments
I have received much assistance from the following people. They either brought mistakes to my attention, or provided additional information that I've incorporated into these specs:

```
Ted Vessenes (tedv@geom.umn.ed)
        I had the THING angles wrong in the original specs.
Matt Tagliaferri (matt.tagliaferri@pcohio.com)
        The author of the DOOMVB40 editor (aka DOOMCAD). I forgot to describe
        the TEXTURE1/2 pointer table in the 1.1 specs. Also, helped with
        linedef types, and provided a good BLOCKMAP algorithm.
Raphael Quinet (quinet@montefiore.ulg.ac.be)
        The author of the NEWDEU editor, now DEU 5, the first editor that can
        actually do the nodes. Go get it. Gave me lots of rigorous
        contributions on linedef types and special sectors.
Robert Fenske (rfenske@swri.edu)
        Part of the team that created the VERDA editor. Gave me a great list
        of the linedef attributes; also helped with linedef types, a blockmap
        list, special sectors, and general tips and suggestions.
John A. Matzen (jamatzen@cs.twsu.edu)
        Instrument names in GENMIDI.
Jeff Bird (jeff@wench.ece.jcu.edu.au)
        Good ideas and suggestions about the NODES, and a blockmap algorithm.
Alistair Brown (A.D.Brown@bradford.ac.uk)
        Helped me understand the NODES; and told me how REJECT works.
Robert D. Potter (potter@bronze.lcs.mit.edu)
        Good theory about what BLOCKMAP is for and how the engine uses it.
Joel Lucsy (jjlucsy@mtu.edu)
        Info on COLORMAP and PLAYPAL.
Tom Nettleship (mastn@midge.bath.ac.uk)
        I learned about BSP trees from his comp.graphics.algorithms messages.
Colin Reed (dyl@cix.compulink.co.uk)
        I had the x upper and lower bounds for node bounding boxes backwards.
Frans P. de Vries (f32devries@hgl.signaal.nl)
        Thanks for the cool ASCII DOOM logo used for the header.

        Thanks for all the help! Sorry if I left anyone out. If you have
any comments or questions, have spotted any errors, or have any possible
additions, please send me e-mail.
```
