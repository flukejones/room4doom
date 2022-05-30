- Line direction determines if the base light is increased or decreased (so 90 degree lines have different brightness)
- There is "extralight" added for weapon flashes
- "lightnum" then indexes a "scalelight" table
- scale table is different for walls vs floor and is built on game start
  + It could be hardcoded for a single res
- Certain player states require a "fixed" colour table