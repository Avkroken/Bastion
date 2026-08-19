# Delade guldfixturer

Filerna här läses av tester på FLERA plattformar samtidigt. Det är hela
poängen: ett nyckelnamn eller en nyttolastform som bara ena sidan förstår
är inte ett teoretiskt problem utan har redan orsakat två tysta
dataförluster vid synk (`forwardAgent` saknades i Swift-modellen,
`jumpHostID` stavades `jumpHostId` av serde).

Nyckeluppsättningen låses av ett test per plattform. `host-wire-format.json`
går ett steg längre och låser VÄRDENA: båda sidor avkodar exakt samma fil
och måste få exakt samma värd. Det fångar sådant en nyckeljämförelse inte
ser — t.ex. att `HostAuth` bär sina fält som `{"keyFile": {"_0": "..."}}`
och att `platform` är en rå sträng.

Ändra aldrig en fixtur för att få ett test grönt. Faller den har trådformatet
ändrats, och då är frågan vad som händer med redan sparade filer hos
användarna.
