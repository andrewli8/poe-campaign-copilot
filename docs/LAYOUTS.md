# Reading the zone layout images

When you enter a zone, the overlay's filmstrip shows one or more small
diagrams for that zone. They are not screenshots or minimaps: each is a
hand-drawn cheat-sheet sketch from the community
["Cheat sheets based on Engineering Eternity" compilation](https://docs.google.com/document/d/1sExA-AnTbroJ-HN2neZiij5G4X9u2ENlC7m_zf1tqP8/edit),
with images by
[Engineering Eternity](https://www.youtube.com/@EngineeringEternity)
(see [CREDITS.md](../CREDITS.md)). Each sketch abstracts one way the zone can
spawn into a plain gray rectangle plus a suggested path, so you can match
it against your in-game map at a glance.

This is the compilation's own legend:

![Layout legend: 1 entrance, 2 waypoint, 3 exit and main path, 4 trial and path, 5 optional area and path, E initial of important NPC](images/layout-legend.png)

The drawings also use a few marks beyond the legend:

| Mark | Meaning |
| --- | --- |
| Gray rectangle | The zone's rough footprint. |
| Green dot | Where the path ends: usually the exit to the next zone, sometimes a boss or quest objective. |
| Green ring | An area circled for the notes, e.g. where quest items are grouped. |
| Gray ring | A landmark area the notes refer to, e.g. the center of Chamber of Sins. |
| Numbers or letters | Ordered stops (1, 2, 3...) or a named spawn (a "V" marks a Voll spawn in The Dried Lake). |

Zone instances are randomly generated, so the sketches show typical cases,
not exact maps. Three patterns cover most zones:

- **Fixed shapes.** Some zones always roll the same overall shape: The
  Tidal Island is always a circle, The Submerged Passage is always linear,
  most roads and forests are "follow the road/wall" linear runs.
- **Orientation tells.** Some layouts are fixed but their direction is
  revealed by a landmark. In The Submerged Passage, small totems stand on
  one side of the waypoint and the exit is always on that side. In The
  Riverways, the Wetlands entrance is always on the opposite side of the
  road from the waypoint.
- **A few known variants.** When a zone shows several images, they are the
  common spawn variants (The Dried Lake ships three, one per Voll spawn).
  Pick whichever matches what you see in game.

Two concrete examples: in The Mud Flats, the green ring marks where the
three quest glands are grouped, connected by little rivers, and the lone
gray dot is the optional Fetid Pool off the route. In The Coast, the gray
spur off the path near the waypoint is the entrance to The Tidal Island
side area.

The step text next to the images carries the matching zone notes. Both
notes and images come from an older patch and carry audit metadata (see the
[content pipeline](DEVELOPMENT.md#content-pipeline) in DEVELOPMENT.md), so
treat them as strong hints rather than guarantees.
