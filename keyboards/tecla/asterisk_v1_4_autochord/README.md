# Asterisk AutoChord

AutoChord turns the Asterisk v1.4 into a bass-and-melody MIDI controller. The
lowest held left note and highest held right note own their exact pitches and
attacks. With both hands playing, the firmware adds tenor and alto between
these anchors. Extra held letter pads guide the harmony. Bass and melody shape
a harmonic phrase automatically; every melody note does not need a new chord.
Thumbs steer its direction, the # bars reinterpret the played notes, and the
stars open richer harmonic choices. There is no internal sound engine.

The note layout uses C major, MIDI channel 1, and fixed velocity 100. Generated
harmony may contain chromatic notes. This layout follows the hardware-mapped
steno profile; physical validation of this revision is pending.

## Install and connect

From the repository root:

```sh
just flash --tecla-config asterisk_v1_4_autochord
```

The coordinator builds the firmware, then waits for the bootloader drive. Hold
the small **B** boot button while connecting USB, then release it. This is the
push button, not the steno `-B` touch pad. Leave the touch pads untouched while
the firmware starts and calibrates; hold the board by its edges.

Select **Asterisk v1.4 AutoChord** in a MIDI monitor or DAW. Route that input to a
software instrument, enable track monitoring, and receive MIDI channel 1. For a
hardware synthesizer, route the controller through a USB MIDI host. No Plover
setup is needed for this profile.

To build without flashing:

```sh
cargo build --release -p tecla-rp2040 --example asterisk_v1_4_autochord --features usb-midi --target thumbv6m-none-eabi
```

## Play

1. Hold a letter pad for bass or melody. With both banks held, two generated
   voices fill between your lowest left note and highest right note. Add more
   pads to suggest harmonic colours while those two anchors remain literal.
2. Press **A** to prepare movement, **O** to intensify its pull, **E** to arrive,
   or **U** to substitute another harmony serving the current direction.
3. Press the **left #** to reinterpret the bass, or **right #** to reinterpret
   the melody. The bar acts immediately; hold it to guide further changes.
4. Hold **left star** to permit borrowing and more adventurous substitutions;
   hold **right star** to permit richer extensions. Then play a note, thumb,
   or #. Both stars combine.
5. Release the note pads to stop, or hold **both # bars** to clear all voices.

```text
       # reinterpret bass                 # reinterpret melody

  G2 A2 B2 C3      * borrow   * extend      A4 B4 C5 D5 E5
  C2 D2 E2 F2                              C4 D4 E4 F4 G4

          A prepare   O intensify   E arrive   U substitute
```

All four thumbs steer further note changes only while held. A press acts
immediately and does not repeat while held. Releasing a thumb, bar, or star is
quiet: it ends that preference without undoing the heard chord or erasing its
unresolved notes. A star press is also quiet; it changes the choices available
at the next musical event. Stars do not tie or freeze individual voices.

When thumbs overlap, the priority is **E > O > A > U**. Adding a lower-priority
thumb makes no extra attack; releasing a higher one quietly exposes the held
lower one. A newly pressed # takes priority over a simultaneous thumb command,
with the held thumb still supplying its steering. A/O/E/U keep their meanings
under either bar; there is no role-selection menu.

Each musical control command articulates the generated tenor and alto, including
their common tones. Held bass and melody continue without another attack;
their own physical note presses articulate them. Note changes retain common
tones where possible. Touch and release each need two consecutive scanner
samples to confirm an edge. Pressing harder does not alter velocity.

The lowest held left note is the bass anchor; the highest held right note is
the melody anchor. These two notes sound exactly as played. Additional held
pads suggest their pitch classes to the harmonic search without sounding extra
manual notes. The instrument stays at four voices: bass, tenor, alto, melody.
A suggestion may appear in a generated voice at another octave, or yield to a
stronger resolution obligation.

Adding an inside finger leaves both anchors steady while giving the middle
voices a new colour to consider. Lifting an extra finger updates that guidance;
lifting a selected anchor hands its role to the next held note. Changing the
shape retains common sounding tones wherever possible. Thumb and bar commands
continue to protect the selected bass and melody from rearticulation.

For a clear comparison, start a fresh phrase with lower-left **S** + right
lower **-G** (C2/E4). Add upper-left **T** (A2): its A suggests Am/C, and the
middle voices become A3/C4 while C2/E4 continue. From another fresh C2/E4
phrase, add upper-left **P** (B2): its B suggests Cmaj7 with G3/B3 inside.
The A2 or B2 pad is a harmonic hint while the lower C2 remains held.

You can also add right-hand pads below the highest held note to guide the
harmony. A new right-hand pad above it becomes the exact melody instead.
Similarly, a new left-hand pad below the bass becomes the exact bass. This
makes rolling onto and off a shape predictable without a separate chord mode.

| Pad group, left to right | Selected pitches | MIDI notes | Scanner indices |
| --- | --- | --- | --- |
| Left lower: S K W R | C2 D2 E2 F2 | 36 38 40 41 | 13 14 15 16 |
| Left upper: S T P H | G2 A2 B2 C3 | 43 45 47 48 | 2 3 4 5 |
| Right lower: R B G S Z | C4 D4 E4 F4 G4 | 60 62 64 65 67 | 17 18 19 20 21 |
| Right upper: F P L T D | A4 B4 C5 D5 E5 | 69 71 72 74 76 | 8 9 10 11 12 |

MIDI note 60 is called C4 here; your DAW may use a different octave label.

| Control | Function | Scanner index |
| --- | --- | --- |
| Left # | Reinterpret the bass within another harmony | 0 |
| Right # | Reinterpret the melody within another harmony | 1 |
| Both # bars | Stop; require fresh presses | 0 and 1 |
| Left star | Permit borrowing and adventurous substitutions | 6 |
| Right star | Permit extensions and suspended chord colours | 7 |
| A | Prepare harmonic movement | 22 |
| O | Intensify directed harmonic pull | 23 |
| E | Arrive at the pending destination when the played notes permit it | 24 |
| U | Substitute another harmony serving the current direction | 25 |

A played C bass may become another chord's third, seventh, or a prepared pedal;
a played melody may become a seventh, ninth, or prepared ornament. These are
contextual choices rather than transpositions of the keyboards. Four voices,
the exact anchors, voice ranges, and existing resolution obligations limit
which choices fit. U remembers recent voicings to discourage short cycles.

The right hand can decorate a steadier harmony. Prepared passing and neighbour
notes can keep the lower voices in place, and a held melody can become a
step-resolving tension during a reharm. Structural landings, bass movement,
and thumb steering can still change the chord. The melody stays exact; low
notes can require the inner voices to move to keep the parts from crossing.

The phrase also responds to how you play. Flowing steps favour continuing the
current harmony; a repeated melody attack or a new attack after a longer gap
can give that note a structural meaning. Arriving at a prepared target can
complete the harmonic movement. The firmware considers timing only when you
make a musical gesture: leaving a note held never starts a timed chord change,
and no tempo or clock needs to be set. Explicit thumb and bar commands retain
their meanings.

To hear the difference, hold lower-left **S** (C2), then play a flowing run on right lower
**-G–-S–-G–-B–-G** (E4–F4–E4–D4–E4). The lower three voices stay put.
After **-G–-S**, keep **-S** held and tap **U**: the same F4 can stay above
a different voicing. Tap **E** to settle the harmony, then return **-S to -G**
to resolve the melody itself.

## Hear automatic movement

With all controls released, play:

1. Lower-left **S** + right lower **-G**: C2/E4.
2. Lower-left **K** + right lower **-S**: D2/F4.
3. Upper-left **S** + the same right **-S**: G2/F4.
4. Lower-left **S** + right lower **-G**: C2/E4.

The harmony moves C–Dm–G7–C without an E press. Roll onto each new pad before
lifting the old one; either hand can move first. During the overlap, the lowest
left note and highest right note sound, and the other held notes guide the
harmony. Lifting the old pad completes the new two-note shape.

For Dm7–G7–Cmaj7, start at C2/E4, then play lower-left **K** + upper **-L**
(D2/C5), upper-left **S** + upper **-P** (G2/B4), and lower-left **S** + the
same **-P** (C2/B4). An inner F stays through G7, then falls to E; your B
continues as the tonic's major seventh.

For a cadential six-four, attack C2/E4, D2/F4, then G2/C5 as separate pairs
(lower S/-G, lower K/-S, upper S/upper -L). A brief release between pairs
preserves the phrase history. Keep upper-left S held and move **-L to -P**:
the melody falls C5–B4 while the alto falls E4–D4. Finish on lower-left **S**
+ upper **-L** (C2/C5). The stars now permit harmonic colour; they do not delay
this resolution.

For a moving bass, hold lower-right **-Z** (G4) and play lower-left **S–K–W**
(C2–D2–E2). The bass connects C to C/E through a dominant inversion.

## Change the harmony while both hands stay put

Hold lower-left **S** + lower-right **-Z**: C2/G4. Tap **O** and release it:
the middle voices change from G3/E4 to B3/F4, forming G7 over the C pedal. Tap
**E** to hear B3–C4 and F4–E4. Both played notes stay fixed.

For a chromatic pull, hold lower-left **S** + lower-right **-G** (C2/E4), tap
**O**, and release it. You hear E7 with C retained in the bass: the inner notes
are G#3/D4. Tap **E** for G#3–A3 and D4–C4. Alternatively, move the left hand
to upper **T** (A2) with E4 held; the same tendencies lead into A minor.

Try each star palette from a fresh phrase: release everything, touch both #
bars, release both, then hold lower-left **S** + lower-right **-G** (C2/E4).
Hold the indicated stars, tap **U**, then release U and the stars. The chord
continues while your two letter pads remain held.

| Stars held for U | First harmony from C2/E4 | Generated middle notes |
| --- | --- | --- |
| Neither | E minor over C | G3, B3 |
| Left | E augmented over C, pulling toward A minor | G#3, C4 |
| Right | Cmaj13 | A3, B3 |
| Both | C13, pulling toward F | A3, Bb3 |

For the left-star chord, keep C2/E4 held, release U, then tap **E**: G#3 rises
to A3 in A minor over C. Moving the bass to upper-left **T** (A2) with E4 held
also reaches A minor. For the both-star chord, release U and move only the
right hand from lower **-G to -S** (E4–F4): Bb3 falls to A3 in F major over C.
Keep the stars held through these arrivals, then release them quietly.

The right star also lets upper-left **S** + upper-right **-D** (G2/E5), then
**O**, produce G13 with B3/F4 inside. Extensions are voiced as four-note shells;
a thirteenth does not mean adding thirteen simultaneous notes.

An established root can also become implied while its guide tones continue.
Hold lower-left **S** + lower-right **-Z** (C2/G4), tap **O**, and release O.
You hear G7 over C, with B3/F4 inside. Keep the C bass and move the right hand
to upper **-D** (E5): B3/F4 stay sounding while E becomes the thirteenth of
that established G harmony. Tap **E** to hear B3–C4 and F4–E4 beneath your
held C2/E5. This continuation needs no star. A fresh pair of notes cannot
invent an absent root; the preceding harmony, retained guide voices, and
destination provide the context.

## A middle voice can make a line

Hold lower-left **S** (C2) and play right lower **-Z**, upper **-L**, then
lower **-Z** again (G4–C5–G4). The tenor descends G3–F3–E3 while the alto's E4
continues. The next played movement advances the prepared passing line;
repeated scans and merely holding the pads do not.

## Stop and reconnect

Both # bars clear sounding notes and harmonic history. USB recovery discards
offline attacks and clears notes before accepting new output. Any note,
thumb, bar, or star held through Stop or recovery must be lifted and pressed
again. Reconnection does not replay held pads or silently restore modifiers.

All live MIDI packets generated by one scan share a 10 ms write deadline.
Cleanup sends one Note Off per scan with its own 10 ms timeout. An unplugged
cable prevents delivery until reconnection, so use the receiving instrument's
panic control if it keeps sounding during the interruption.

## Check the setup

- If nothing sounds, inspect the MIDI monitor, receiving instrument, channel,
  and track monitoring. Hold note pads to sound them.
- If pads misbehave, reboot with the touch pads untouched, including their
  backs. Keep the board flat. The small **R** push button reboots it.
- After Stop or USB interruption, lift the controls before trying them again.

Before calling this profile physically validated, check every selector and
control, transitions between one and two hands, both star permissions, Stop,
and USB disconnect/reconnect while notes and modifiers are held. Record the
firmware revision, host, receiving instrument, and observed results.
