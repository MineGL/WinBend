# Generating the "real lid" shots with Gemini (Veo)

Open the Gemini app → **Video** (Veo). Ask for **vertical 9:16** and **8 seconds** where the
option exists; otherwise generate 16:9 and let `make-live.ps1` letterbox it. Generate each shot
two or three times and keep the take where the screen content stays stable and the hand motion
is clean. Save the clips into `promo\clips\`.

The on-screen effect Veo invents will only approximate WinBend. That is fine: the script cuts
straight from the Veo shot to the real engine render, which is the honest part.

## Shot 1 — closing the lid (hero shot)

> First-person point of view, sitting at a desk, a modern silver Windows laptop directly in
> front of the camera. A hand reaches from the bottom of the frame and slowly closes the laptop
> lid. As the lid tilts down, the picture on the screen tilts backward with it, bending and
> folding down like a sheet of paper hinged at the bottom edge, darkening toward the top,
> settling flat just before the lid closes. Soft evening window light, shallow depth of field,
> minimal desk, no text on screen, no logos. Smooth, slow, realistic motion. 8 seconds,
> vertical 9:16.

## Shot 2 — opening the lid (the snap-back)

> First-person point of view of a closed modern silver Windows laptop on a minimal desk. A hand
> lifts the lid slowly. As the lid rises, the desktop picture on the screen unfolds upward from
> the bottom edge like paper lifting, brightening as it becomes flat, then the screen shows a
> calm desktop wallpaper. Soft daylight, shallow depth of field, no text on screen, no logos.
> Smooth realistic motion. 8 seconds, vertical 9:16.

## Shot 3 (optional) — over-the-shoulder / side angle

> Cinematic side angle at desk height of a modern silver laptop, a person's hand closing the
> lid slowly. The screen content folds down with the lid like a sheet of paper hinged at the
> bottom, casting a soft glow on the keyboard that fades as it closes. Dark room, single warm
> desk lamp, product-commercial lighting, shallow depth of field, no text, no logos. 8 seconds,
> vertical 9:16.

## Tips

- Add "no on-screen text, no logos, no watermark" if Veo puts UI or brands on the screen.
- If the screen content flickers, ask for "a calm static mountain wallpaper on the screen".
- If hands look wrong, ask for "hand only partially visible, fingertips on the lid edge".
- For a Windows feel, add "the laptop has a Windows logo key on the keyboard" (but not on screen).

## Then

```powershell
powershell -ExecutionPolicy Bypass -File promo\make-live.ps1 -Clips promo\clips\close.mp4,promo\clips\open.mp4 -Music "C:\Users\MineGL\Downloads\Cutscene Crush - Blue Deer Studio.mp3"
```

Output: `promo\out\winbend-live.mp4` (1080x1920). Requires the fold frames from
`make-short.ps1` (any source) for the "rendered live" segment.
