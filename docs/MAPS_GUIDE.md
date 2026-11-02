# Maps: a quick guide

Every saved settlement can have a map. Cities and Capitals get one map per
district. Nothing happens on its own: you decide when to draw, and nothing
you do on a map changes the settlement itself.

## 1. Draw a map

1. Open **Saved Settlements** and pick a settlement.
2. Choose the **Map** tab.
3. Press **Generate map**. For a City or Capital, press **Generate** on each
   district you want (or **Generate all missing districts**).

The map is saved. Next time it opens exactly as it was.

## 2. Move around

| To…             | Do this                                              |
| --------------- | ---------------------------------------------------- |
| Zoom            | Scroll the mouse wheel, or the **+** / **−** buttons |
| Move the map    | Click and drag                                       |
| See everything  | **Fit map**                                          |
| Start over      | **Reset view**                                       |
| Change district | Click its tab, or type in **Search districts**       |

Each district remembers where you left it.

## 3. Look at a building

Click a building. The **Location Inspector** on the right shows what it is:
type, condition, who lives there and a short description.

The first time you click a building, its details are made up for you and
saved. Click it again tomorrow and they are the same.

- **Edit details**: change anything. Your changes are marked **Edited**.
- **Regenerate…**: roll new details. Your own changes are kept unless you tick
  the box to replace them.
- **Reset…**: undo your changes and go back to what was rolled.

## 4. Mark important places

The settlement's **Notable Locations** (its inns, temples, shops and so on)
can be put on buildings.

1. Click a building.
2. Press **Make Notable Location**.
3. Pick a location from the list (or **Custom entry…** for your own, such as
   a castle).
4. Press **Make Notable Location** again to confirm.

A numbered pin appears on the map, and the same number appears in the
**Legend**. Numbers never change, even if you add or remove others.

- To move it: select its building and press **Reassign…**, then click the new
  building.
- To take it off: **Unassign…** (the location and its number are kept).
- To rename it: **Edit location**.

## 5. Add your own pins

Press **Add point** in the map toolbar, then click the spot and give it a name
(for example "Old well" or "Ambush site"). Points use diamond pins, so you can
tell them from Notable Locations.

## 6. Find things

- **Legend**: everything numbered on this map. Click an entry to jump to it.
- **Directory**: every location and point in the whole settlement. Click one
  to jump there, even if it is in another district.
- **Building directory**: every building, searchable.

## 7. Save pictures and backups

- **Export…** (map toolbar) saves the map as a **PNG** picture, an **SVG**
  drawing or **map data**. For a City you can export one district or all of
  them (one file each).
- **Export** (Saved Settlements) saves the whole settlement, with all its
  maps, to one file. **Import settlement…** brings such a file back as a new
  settlement.

Files go to the **Settlement Generator** folder in your **Documents**.

## 8. In the terminal version

Open **Saved Settlements** (View), choose a settlement, then **Maps**. You can
do everything above except see the drawing itself. Instead you work with
building numbers and coordinates.

- **Buildings…**: type to search, Enter to open.
- When editing a field, leave it empty to make it blank, or type `=` to get
  the rolled value back.
- **Points…** asks for a position as `x y` in metres from the middle of the
  map (x to the east, y to the south).
- Press `i` on the Saved Settlements list to import a settlement file.

## Good to know

- **Redrawing** a map gives it new buildings. Your old building details are
  not put on the new buildings, and Notable Locations need placing again.
- **Delete map** (a district's) tells you exactly what will go with it before
  anything is deleted.
- If some saved map data is ever damaged, the map shows a warning with
  **Discard damaged data…**. Everything else stays safe.
