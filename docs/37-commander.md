# 37 · The Commander on the Field

Idea 12 of the [restart plan](29-restart-plan.md) (thread T4, with [28](28-soldier-combat.md) stage 5): each side's general rides with one of its regiments, speaks to his men before the lines meet, steadies those near him, and when he falls the men around him panic. Until heroes reach Field Battles ([05](05-combat.md#heroes)), the general is a soldier of the simulation, not a hero with levels and items; when heroes come, the hero takes his place and keeps these rules.

## Who he is

- **Field Battles only.** Conquest gets heroes later, and its gentler morale rules stay as they are.
- **One general for each player**, picked within a second or two after that player has a regiment on the field (once deployment is over), and never replaced. He goes to the best fighting regiment: one that fights in melee rather than shooting if there is one, then the highest morale plus melee attack plus defence, then the most soldiers, then the first in the regiment list. A running regiment can't take him. He leads only his own player's regiments, not an ally's.
- **He is a man in the ranks.** He stands at the centre of the second rank, or at the end of the rank when there is only one. When the regiment changes its frontage, or a man ahead of him falls and he steps up into the gap, he goes back to that place on the next tick. So blows from the front reach him only once his regiment has lost enough men that he is the last man behind its front rank: then each man who falls in front pulls him up into the gap for a moment. Arrows, which strike the back of a block first, reach him at the same point. A charge into the rear of a line two ranks deep reaches him at once: catching a general from behind is how to kill him. He has three times the hit points of one of his men (better armour, and the best fighter there), and dies as they do, when his own hit points run out. With a general no tougher than his men he fell in nearly every battle of the computer players' arena; at three times, the winning side's general falls in about half of them. If he is the man his regiment sends out as its champion and loses, he falls.

## What he does

- **Leadership.** Every regiment of his within 20 tiles of his regiment, his own included, gains 1 morale point a second, scaled by the battle's pace like every steady drain (0.7 a second), in a fight or out of one, while his regiment is not routing. That covers a third of the flank shock or half the drain of being outnumbered two to one: a line near the general bends later. It never lifts a regiment above its ceiling.
- **The speech.** The first time an enemy regiment his side can see comes within 45 tiles of his regiment, while it is neither fighting nor routing, he speaks. Every regiment of his within 20 tiles gains 5 morale and a ceiling 5 higher for 60 seconds, then the ceiling falls back.
- **When he falls.** Every regiment of his within 25 tiles of where he fell loses 10 morale, and every other regiment of his loses 4, down to no less than a tenth of a point, as champions' losses do: their own next check decides whether they break. Enemies fighting his regiment gain 5. His side has no leadership for the rest of the battle. His own regiment is usually near breaking by then, so a rout often follows, and its rout shakes its neighbours too.
- **The Concord's undead** never rout, so morale means nothing to them. Instead, while he leads them, undead within 20 tiles strike 10% harder, driven on by the will that raised them; when he falls, those within 25 tiles falter into full disorder (20% less damage dealt, 20% more taken, fading over 3 seconds). Without this a Concord army, all Bone Pikes in a Field Battle today, gained nothing from its general while every other faction did, and lost far more often (the Choir beat it 73 times in 100).
- **If his regiment routs** he runs with it and leads nobody until it rallies. If it is destroyed, or removed, he falls with it.

## On screen

- The general is drawn a little taller than his men and in a lighter shade of his side's colour, so a player can find him without a marker. Selecting his regiment says "Your general rides here: guard him".
- **The speech:** he raises his weapon and most of the men within reach cheer, a few at a time, over a few seconds; the battle panel says "*Side*'s general rallies his army" for six seconds.
- **The fall:** the battle panel says "*Side*'s general has fallen!" for eight seconds, and the men of the regiments that panic flinch as one.

## What comes later

- A command to move the general to another regiment (a new order, so the replay format and net protocol change), and computer players that keep their general out of harm. Until then a human who guards his general well has a small edge over computer players.
- The faction twist from [05](05-combat.md#morale): the Brood lose more from losing a nearby hero. The simulation doesn't know factions yet, so it waits for heroes.
- Matched kills, and a longer death move for the general, with the fight library ([28](28-soldier-combat.md) stages 3 and 4).
