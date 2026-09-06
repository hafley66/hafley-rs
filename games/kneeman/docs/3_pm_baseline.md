# Project M reference baseline

Working target: original Project M 3.6, following the recovered `pm-falcon-kit.md` title.
This version is a working assumption pending an authenticated release artifact. PM-CC,
3.6.1 development builds, Project+ and Melee retain distinct labels in behavior receipts.

## Pinned text reference

Repository: https://github.com/Project-M-CC/Project-M-CC
Revision: `6e63ffa920d45e9d0236edbec4bf43057e7b3e2d`, dated 2016-07-26.
Local sparse checkout: `/Users/chrishafley/projects/kneeman-lines/7_project_m_cc`.
Only README and three text codesets are materialized. No game binaries or character assets
were downloaded, executed or added to Game3. The README describes a community build based
on 3.6.1 with optional 3.6 fighter reverts; its root character assets cannot be assumed to
match the original 3.6 release.

| File under `[Dev Resources]` | Encoding | SHA-256 |
| --- | --- | --- |
| codes-3_6.txt | ASCII | 0a2f08b47e06306dd675dc901c0e35c88fddd1b82fd95ad5b43d68004902142a |
| codes-3_6-wifi.txt | ASCII | 2e106e3aeb2ce7afdddf03d37580e4c8b8d780b818a013f1fb5c4a9ce76d4ab7 |
| codes-3_61.txt | UTF-16LE | 1065affec29049d4096ab802f5cd69f4e38cc24c6a0b4ea21f6920ffab9126ba |

The archived 3.6 text begins with `Unknown Code` and has no mechanic labels. Labels below
come from the annotated 3.6.1 file. Complete consecutive code-row blocks match the archived
3.6 file exactly after removing the annotation prefix. This proves shared bytes in these
files, without establishing complete release identity, runtime timing or fighter attributes.

| Candidate mechanic | 3.6.1 label line | Code rows | Matching 3.6 line | Game3 inspection |
| --- | ---: | ---: | ---: | --- |
| Last-frame jump direction | 3064 | 10 | 2766 | `za_warudo.rs` JumpSquat reads direction throughout squat and at takeoff; exact source semantics unverified |
| Jump-canceled grab | 3803 | 9 | 3283 | JumpSquat branch handles up-smash and takeoff, without a grab transition; candidate implementation gap |
| L-canceling, part 1 | 4333 | 46 | 3781 | One patch part only; cannot establish full behavior or window |

Reproduce from `games/kneeman`:

```sh
node tools/0_pm_codes_receipt.mjs /Users/chrishafley/projects/kneeman-lines/7_project_m_cc
```

The checker rejects unexpected source hashes and requires one exact occurrence of each
complete block. It reads text only and emits line receipts; it does not interpret Gecko/PSA.

## Next mechanic: jump-canceled grab

1. Interpret the nine-row patch with the corresponding Brawl action data and addresses.
2. Determine input priority, permitted squat ticks, airborne boundary and momentum behavior.
3. Record Falcon run -> jump -> grab input sequences, including one tick before/at/after takeoff.
4. Assert state transitions and full-state replay in Game3; then expose the fixture in the debugger.
5. Compare a reference execution before marking PM parity. A matching transition name is insufficient.

## Existing evidence limits

Recovered `pm-falcon-kit.md` asserts that missing PM changelog entries make Melee values
PM ground truth. That inference is unverified and must not supply acceptance expectations.
Current `src/v1/chars/falcon.rs` inherits KneeMan's kit and replaces only special slot 2
with Falcon Dive. The generic tuning still labels jumpsquat as universal three frames;
the recovered document claims Falcon four frames. Character timing needs direct data evidence.
No original release PAC attributes, Falcon hitbox scripts or complete PM runtime traces have
been validated here. Original 3.6 artifact authentication and per-character extraction remain open.
