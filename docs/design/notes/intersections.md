# Intersection-related design notes

## Nice diagrams

http://streetsillustrated.seattle.gov/design-standards/intersections/pedcrossing/

## Stop sign editor

Stop signs are FIFO, except that many intersections only have a stop sign for
some sides. Going straight on the priority roads is immedite, and left turns
from those priority roads also take precedence over the low-priority roads. So
should the stop sign controller mark individual turns as priority/not, or
individual roads, with implied semantics for left turns? There are really 3
priorities if turns are considered...

Figuring out nonconflicting roads seems tricky. For now, going to have a
complicated UI and let individual turns be classified into 3 priority classes.
First group can't conflict, second and third groups can conflict and are FIFO.
Will probably have to revisit this later.

## Stop signs

How to depict stop signs? Each driving lane has a priority... asap go or full
stop. Turns from go lanes might be yields, but shouldn't need to represent that
visually.

- Easy representation: draw red line / stop sign in some driving lanes. Leave the priority lanes alone.
- Harder: draw a stop sign on the side of the road by some lanes. Won't this look weird top-down and at certain angles?

## Traffic signals

- per lane would be weird.
- drawing turn icons as red/yellow/green is pretty clear...
- could draw an unaligned signal box with 3 circles in the middle of the intersection, but what does it represent? maybe just an initial indicator of what's going on; not full detail.
- similarly, draw a single stop sign in the middle of other intersections? :P

- http://streetsillustrated.seattle.gov/design-standards/intersections/its/ probably has hints on cycle duration
- https://ops.fhwa.dot.gov/publications/fhwahop08024/chapter4.htm
	- ring and barrier diagrams
- https://www.webpages.uidaho.edu/TrafficSignalSystems/traffic/instructor/ch3a.pdf
- http://iamtraffic.org/evaluation/the-six-way/

ah, think of the real world for the UI. Light per incoming lane. Sometimes need
to show a green/yellow/red arrow. Could similarly show the ped stop hand or the
walking figure, probably at both tips of a sidewalk.

- https://www.slideshare.net/hronaldo10/lecture-06-signalized-intersections-traffic-engineering-profusama-shahdah
	- especially slide 11 has the best notation for protected and permited turns!
	- slide 12 shows a nice table

### Yielding

Similar to a stop sign, there are turn priorities for cycles. Some turns have
right-of-way, others need to yield, but can go if there's room

## Intersection policies for pedestrians ##

Before figuring out how pedestrians will deterministically use intersections alongside cars, recall how cars currently work...

- ask all cars for next move (continue on same thing, or move to a turn/lane)
- using fixed state, adjust some of the moves that dont have room to move to a new spot to wait instead
- serially ask intersections if a car can start a turn
- serially make sure only one new car enters a lane in the tick
	- shouldnt the intersection policy guarantee this by itself?
- very awkwardly reset all queues from scratch

How did AORTA do it?

- agent.step for all of em (mutate stuff)
	- enter intersections, telling them. must've previously gotten a ticket
- let all the agents react to the new world (immutable, except for IDEMPOTENTLY asking for turn)
	- here we ask for tickets, unless we've already got one
- same for intersections
	- grant them here

aka basically yeah, the simple:

- agents send a ticket during the planning phase?
- intersections get a chance to react every tick, granting tickets
- during the next action phase, an agent can act on the approved ticket?

good pattern in intersections:
- a sim state that the rest of the code interacts with for ALL intersections. rest of code doesnt see individual objects.
- that manager object delegates out most of the logic to SPECIALIZED versions of individual objects and does the matching
	- no need for this to exist on the individual IntersectionPolicy object

How to share common state in intersections?
- if it's just the accepted set, have a parallel array and pass it into step()
	- data locality gets ruined, this is ECS style, bleh
- have a common struct that both enum variants contain
	- still have to match on enum type to operate on it commonly!
- have one struct that then contains an enum
	- when delegating to specialized thing, can pass this unpacked thing down, right?

Seeing lots of deadlock bugs from accepting non-leader vehicles. For now,
switch to only considering leader vehicles, and later maybe relax to anybody
following only accepted vehicles.

Leader vehicle is a bit vague; could be leader on current queue, which is still a bit far away.

## Stop sign priority

Use OSM highway tags to rank. For all the turns on the higher priority road, detect priority/yield based on turn angle, I guess.

## Handling complicated intersections with tiny roads

AORTA's super-roads

https://www.mapbox.com/mapping/mapping-for-navigation/modeling-intersections-for-map-navigation/
https://wazeopedia.waze.com/wiki/Global/Junction_Style_Guide/Intersections
https://wiki.openstreetmap.org/wiki/Junctions

http://www.sumo.dlr.de/userdoc/Networks/Building_Networks_from_own_XML-descriptions.html#Joining_Nodes
- instead of looking for short roads, clip road lines to intersection geometry and then see if they're short (10m)

Maybe start by hardcoding the things to merge. Don't worry about detection yet. r257

- When to do the operation?
	- do it before making lanes ideally...
	- first, make lanes separately from making roads, so intersection geometry can move up.
	- could then even merge the trim lines step? hmm.
	- or even do it to raw data? is that possible?


- r257 example
	- extend two roads with points from the tiny road. delete i283
	- make sure the road attributes match up
	- do this first; preprocess raw_data.

Ah, the problem with just extending geometry... I think turns need to be
polylines sometimes. Need to do the merging all the way at the end,
unfortunately...

### Attempt v2

Could use https://data.seattle.gov/Transportation/Intersections/e7db-mhd7 as a
source of truth? Match all OSM intersections to one of these.

Shelby and 23rd...
- want a big intersection to cover the two tiny horizontal roads
- want to glue turns together from the original things (turn+lane+turn)

- input is still a tiny road to collapse.
- the two intersections it's connected to will logically become one
- redo the intersection and lane geometry
- use the original turns to create composites and just use those; dont recalculate turns

- probably need a HalfMap (in between raw and Map) or something. the order...
	- raw intersections -> real intersections
	- raw roads -> real roads and lanes
	- assign border intersections
	- make the intersection polygons
	- trim lanes
	- populate turns

	- then do the merging magic
	- need to fix up IDs everywhere

	- ... rest of map construction proceeds

## Live editing

Traffic signal changes can happen live, because weird changes will just look
like brief blips of overtime. Same for stop signs, I think...

## Roundabouts

Make em look nice.

- 26th and Boyer (i333)
	- there's a single highway=residential, junction=roundabout way
	- find all the other ways incident to it
	- the road splitting we do in convert_osm isn't appropriate at all
	- geometrically, this could just be a single intersection
	- could hint that it's a roundabout and make new turns later, or cheat and treat it as an all-way stop sign for now
- Lynn and Boyer (i135)
	- way more complicated. there's a junction=roundabout way, but also a bunch of... other stuff

Lynn and Boyer needs more work, but now it seems like it's just merging short
roads. If we just cherry-pick some roads to destroy and make enveloped by the
intersection, will it work? Will reasonable turns result? Or do we have to
retain hints in the raw Intersection about things that should be connected and
the original path inside the intersection that should do it?

## v3 of short road handling

- soln1: lets try MANUALLY merging intersections
	- how can we specify in a map-agnostic way? OSM ids? intersection name?
	*** none of this is looking good. manually draw what i want.
- soln2: make that road longer by not requiring perpendicular endings?
- soln3: disallow that weird sequence of turns. c615. north on 23rd, left on madison
	- does OSM have turn restrictions here? nope.
	- what's reality? its a ped island.
	- this feels brittle regardless
	- right from madison to 23rd southbound MUST happen from oneway, not at the light
- soln4: more complicated sim to lock a sequence of turns
	- pushes the problem downstream...
- soln5: loosen the sim rules about proceeding through "blocked" intersection
- soln6: manual map editor, damnit. blow up roads. define new ones by dragging points. copy over md.
	- try operating on InitialMaps
