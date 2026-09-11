# Generating the "real lid" shots with Gemini (Veo)

## Recommended: black screen + compositing

Ask Veo only for what it is good at, a laptop whose lid closes on its own with the screen
**switched off**. `make-composite.ps1` then tracks the black screen in every frame and projects
WinBend's real fold onto it, with the fold amount following the lid angle. That is the honest
version of the effect, and it looks right because the perspective of the physical screen is real.

> Product commercial shot, camera at desk height facing a modern thin silver laptop with a light
> grey keyboard on a clean light wooden desk, in a bright room. The laptop screen is completely
> switched off, pure black, glossy. The lid slowly closes by itself in one continuous smooth
> motion, no hands, no people, the lid never reopens. Static camera, soft daylight, shallow depth
> of field, no text, no logos, no watermark. 8 seconds, vertical 9:16.

Opening shot: same prompt with "starts closed and slowly lifts open by itself in one continuous
motion". The screen must stay pure black the whole time; add "the screen stays off and black,
no reflections of light" if Veo lights it up.

Then:

```powershell
powershell -ExecutionPolicy Bypass -File promo\make-composite.ps1 -Clip promo\clips\close-black.mp4 -Style silk
powershell -ExecutionPolicy Bypass -File promo\make-composite.ps1 -Clip promo\clips\open-black.mp4 -Style silk
powershell -ExecutionPolicy Bypass -File promo\make-live.ps1 -Clips promo\clips\close-black-winbend.mp4,promo\clips\open-black-winbend.mp4 -Music "C:\Users\MineGL\Downloads\Cutscene Crush - Blue Deer Studio.mp3"
```

If the desk or keyboard gets picked up as "screen", lower `-Threshold` (e.g. 30) so only truly
black pixels count. If the screen is missed, the fix is the prompt, not the threshold: ask for a
darker screen and a lighter room.

## Alternative: let Veo invent the on-screen effect (less faithful)

Open the Gemini app → **Video** (Veo). Ask for **vertical 9:16** and **8 seconds** where the
option exists; otherwise generate 16:9 and let `make-live.ps1` letterbox it. Generate each shot
two or three times and keep the take where the screen content stays stable and the lid motion
is clean. Save the clips into `promo\clips\`.

No hands in any shot: the lid moves on its own, the way product commercials show it. Hands are
where video models fall apart, and the empty frame reads more premium anyway.

The on-screen effect Veo invents will only approximate WinBend. That is fine: the script cuts
straight from the Veo shot to the real engine render, which is the honest part.

## Shot 1 — closing the lid (hero shot)

> Product commercial shot, camera at desk height facing a modern thin silver laptop on a clean
> minimal desk, lid open. The laptop lid slowly closes by itself, smoothly, as if on a motorized
> hinge, no hands, no people. As the lid tilts down, the picture on the screen tilts backward
> with it, bending and folding down like a sheet of paper hinged at the bottom edge, darkening
> toward the top and settling flat just before the lid shuts. Soft evening window light, shallow
> depth of field, static camera, no text on screen, no logos, no watermark. Slow, smooth,
> realistic motion. 8 seconds, vertical 9:16.

## Shot 2 — opening the lid (the snap-back)

> Product commercial shot, camera at desk height, a closed modern thin silver laptop on a clean
> minimal desk. The lid slowly lifts open by itself, smoothly, no hands, no people. As the lid
> rises, a desktop picture on the screen unfolds upward from the bottom edge like paper lifting,
> brightening as it becomes flat, ending on a calm static mountain wallpaper. Soft daylight,
> shallow depth of field, static camera, no text on screen, no logos, no watermark. Slow,
> smooth, realistic motion. 8 seconds, vertical 9:16.

## Shot 3 (optional) — side angle in a dark room

> Cinematic side angle at desk height of a modern thin silver laptop in a dark room lit by a
> single warm desk lamp. The lid slowly closes by itself, no hands, no people. The screen content
> folds down with the lid like a sheet of paper hinged at the bottom edge, its glow on the
> keyboard fading as it closes. Product-commercial lighting, shallow depth of field, slow subtle
> camera push-in, no text, no logos, no watermark. 8 seconds, vertical 9:16.

## Shot 4 (optional) — top-down, lid closing toward camera

> Top-down overhead shot of a modern thin silver laptop on a wooden desk, lid open toward the
> camera. The lid slowly closes by itself, no hands, no people; the screen picture folds down
> with it like paper, darkening as it turns away. Soft natural light, static camera, no text, no
> logos, no watermark. 8 seconds, vertical 9:16.

## Tips

- If the lid stutters or reverses, add "one continuous motion, the lid never reopens".
- If the screen content flickers, ask for "a calm static mountain wallpaper on the screen".
- If Veo adds UI or brands, repeat "no text on screen, no logos, no watermark" at the end.
- If a hand still appears, add "the room is empty, nobody is present".
- For a Windows feel: "a Windows logo key is visible on the keyboard" (keyboard only).

## Then

```powershell
powershell -ExecutionPolicy Bypass -File promo\make-live.ps1 -Clips promo\clips\close.mp4,promo\clips\open.mp4 -Music "C:\Users\MineGL\Downloads\Cutscene Crush - Blue Deer Studio.mp3"
```

Output: `promo\out\winbend-live.mp4` (1080x1920). Requires the fold frames from
`make-short.ps1` (any source) for the "rendered live" segment.
