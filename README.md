## About
zeroTremor is an accessibility engine that learns how you physically interact with a computer, then modifies interfaces around your motor abilities in real time.

Imagine trying to click a tiny button when your hand physically won’t stay still.

For people with essential tremor, Parkinson’s, or other motor impairments, using a computer can turn simple actions—clicking a link, filling out a form, or selecting a menu—into frustrating precision tasks.

That’s why we built **zeroTremor**, an adaptive accessibility system that works at both the operating-system and browser level.

At the OS level, zeroTremor reads a user’s physical mouse input, suppresses the shaky raw movement, filters out tremor-like motion, and replaces it with a smoother synthetic cursor—all while preserving intentional movements like fast flicks.

But smoothing the cursor only solves half the problem. Our Chrome extension understands what’s actually on the webpage. It detects interactive elements like buttons, links, and inputs, and dynamically expands their effective hitboxes based on the user’s individual tremor profile.

The two layers communicate through a native bridge, so one calibration can personalize both the cursor stabilization and the browser interface.

Instead of asking someone with a motor disability to adapt to their computer, **zeroTremor makes the computer adapt to them.**

### Synthetic Tremor Test Results

#### WITHOUT zeroTremor
10 misses

1.64 s average target time

2.25× path efficiency

#### WITH zeroTremor
1 miss

1.13 s average target time

1.83× path efficiency

90% fewer misses

31% faster target acquisition

18% more efficient cursor paths
