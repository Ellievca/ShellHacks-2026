## About

**zeroTremor** is an adaptive accessibility project designed to make computer interaction easier for people with hand tremor and other motor impairments.

Imagine knowing exactly which button you want to click — but your hand physically won't stay still long enough to land on it.

For people with essential tremor, Parkinson's disease, and other conditions affecting motor control, everyday web interactions such as clicking links, filling out forms, and navigating menus can become frustrating precision tasks.

### SteadyUI

Our working prototype, **SteadyUI**, is a standalone Chrome extension that adapts webpage interactions to the way an individual user moves.

SteadyUI begins with a short calibration exercise. As the user moves between targets, it measures pointer-path efficiency, missed clicks, and target-acquisition time to generate a personalized assistance profile.

On any webpage, SteadyUI then:

- Detects interactive elements such as buttons, links, inputs, and controls.
- Analyzes pointer trajectory, distance, direction, and approach behavior to estimate the user's intended target.
- Dynamically expands the effective hit area of likely targets based on the user's calibrated assistance profile.
- Rescues near-miss clicks when the user's intent is sufficiently clear.

The webpage itself does not need to be modified.

### Synthetic Tremor Testing

To test SteadyUI under repeatable motor-control difficulty, we also built a configurable tremor simulator in Rust.

The simulator produces synthetic tremor-like pointer movement that lets us compare interaction performance under controlled conditions without representing the data as real patient or clinical data.

### Synthetic Tremor Test Results

| | Without SteadyUI | With SteadyUI |
|---|---:|---:|
| Missed targets | 10 | **1** |
| Average target time | 1.64 s | **1.13 s** |
| Path efficiency ratio | 2.25× | **1.83×** |
| Adaptive assistance profile | 36 px | 28 px |

**90% fewer missed targets**

**31% faster target acquisition**

**18% reduction in path-efficiency ratio**

These results come from our synthetic tremor test environment and are intended as prototype validation, not clinical evidence.

Instead of asking someone with a motor disability to become more precise, **zeroTremor makes the interface more forgiving.**
