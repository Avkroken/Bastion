# Gränssnittsprototyp

`bastion-gui.html` är en interaktiv prototyp av Bastions gränssnitt: värdlista
med taggar, dashboard, terminal, Docker, SFTP, snippets, nycklar och synk.
Plattformsväljaren byter fönsterchrome — macOS, Windows (WinUI), Linux (GTK4)
och telefonläge för iOS/Android med tangentbordsrad.

Den är ett designunderlag, inte en klient: en webbsida kan inte öppna SSH.
Terminalen är simulerad och svarar med samma data som panelerna visar
(`help` listar kommandona). Innehållet följer VISION.md — samma
funktionsuppdelning, samma svenska etiketter som `App/`.

Öppna filen direkt i en webbläsare, eller publicera den som artefakt.
