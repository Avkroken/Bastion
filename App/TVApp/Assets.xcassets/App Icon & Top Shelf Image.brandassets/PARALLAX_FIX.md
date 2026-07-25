# Parallax-ikon fix (TODO)

Front/Middle/Back-lagren i `App Icon.imagestack` och `App Icon - App Store.imagestack` är för närvarande identiska PNG-filer, vilket ger ingen faktisk parallax-effekt.

## Vad behöver göras

För varje upplösning (small-1x, small-2x, large-1x, large-2x):

1. **Front.imagestacklayer** - Behåll nuvarande bild (huvudlager)
2. **Middle.imagestacklayer** - Skapa en något bakåtflyttad variant (t.ex. lägg till subtil skugga, ändra ljussättning)
3. **Back.imagestacklayer** - Skapa en mer bakåtflyttad variant (t.ex. ännu mer skugga, annan bakgrundsfärg)

## Varför

tvOS-använder dessa lager för att skapa en 3D-effekt när användaren navigerar med Siri Remote. Identiska lager ger ingen visuell feedback.

## Anteckning

Detta kräver grafiska verktyg (Photoshop, Sketch, Figma, etc.) för att skapa varianterna. Det är en kosmetisk fix som inte blockerar funktionalitet.