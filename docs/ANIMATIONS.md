# Animations

* Every animation planned for the cat, & what each one is for.
* The finished frames are in [FRAMES.md](FRAMES.md).
* How to draw them in Blender: [ANIMATION.md](ANIMATION.md).

## Contents

* [Resting & Moving](#resting--moving)
* [Play](#play)
* [Reactions](#reactions)
* [Props](#props)
* [Ideas, Not Planned Yet](#ideas-not-planned-yet)
* [Draw First](#draw-first)

## Resting & Moving

| Animation | Frames | Loops | Used For |
| --- | --- | --- | --- |
| `sleep` | 7 | Yes | Naps & night. |
| `wake` | 5 | No | Waking up. |
| `sit` | 8 | Yes | Resting & watching, with a blink. |
| `sit_down` | 4 | No | Standing to sitting. Played backwards to stand up. |
| `walk` | 8 | Yes | Walking & going home. Moves along the bar. |
| `run` | 8 | Yes | Running & chasing the ball. Moves along the bar. |
| `turn` | 2 | No | Turning around. |
| `eat` | 8 | Yes | Meals & treats, at the bowl. |

## Play

| Animation | Frames | Loops | Used For |
| --- | --- | --- | --- |
| `crouch` | 2 | No | Getting low before a pounce. |
| `pounce` | 5 | No | Jumping at the ball. Moves along the bar. |
| `bat` | 4 | No | Swatting the ball. |

## Reactions

| Animation | Frames | Loops | Used For |
| --- | --- | --- | --- |
| `look_up` | 2 | No | When you type. |
| `flinch` | 2 | No | When a command fails. |
| `perk_up` | 3 | No | When a coding agent starts. |
| `purr` | 8 | Yes | When you run `meowsh pet`. |

## Props

| Prop | Used For |
| --- | --- |
| `bowl` | Meals & treats, at the cat's home spot. |
| `ball` | Play. It rolls & stays where it stops. |

## Ideas, Not Planned Yet

| Animation | Frames | What It Is | Idea |
| --- | --- | --- | --- |
| `wiggle` | 6 | The butt wiggle between `crouch` & `pounce`. | Worth adding. Makes play feel alive. |
| `petted_in` | 2 | Eyes closing as the cat is petted, before `purr`. | Worth adding. Eases into the purr. |
| `lean` | 3 | Leaning down from sitting. | Worth adding. Could lead into `eat`. |
| `stand` | 6 | Lying down to standing. | Likely not needed. `sit_down` played backwards covers standing up. |

## Draw First

* `sit` & `walk`. They are enough to start Phase 4.
* Together they cover one still pose & one moving pose, so problems show up early.
* Then `sleep`, `wake`, `eat` & the bowl.
* Then play, run, turn & the reactions.
