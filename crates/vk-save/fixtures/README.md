# Fixture binarie

Due file di salvataggio **reali**, anonimizzati. Sono ciò che autorizza le
scritture su `rksys.dat` e `RFL_DB.dat` (vedi `docs/decisions.md` §D-012): un
salvataggio sintetico dimostra soltanto che il codice è coerente con sé stesso,
non che il formato sia stato capito.

| File | Origine | Dimensione | Contenuto |
| --- | --- | --- | --- |
| `rksys.dat` | salvataggio Mario Kart Wii, regione NTSC-J | 163 840 B (`0x28000`) | 4 licenze, 31 amici, record dei tempi |
| `RFL_DB.dat` | database Mii di Dolphin | 127 456 B (`0x1F1E0`) | 22 Mii in 21 identificativi distinti |

## Cosa è stato tolto

`anonymize.py` sostituisce **ovunque compaiano nel file**, non solo nei campi
noti:

- i nomi delle licenze, dei Mii e dei creatori;
- i profile ID, cioè i friend code, con il checksum ricalcolato;
- i Mii ID e i system ID, che identificano la console che ha creato il Mii.

La sostituzione è globale perché il salvataggio ne tiene copie anche nei record
dei tempi e in regioni di cui non si conosce il significato. Lo script verifica
che nessun valore originale sopravviva prima di scrivere il risultato.

Tutto il resto — struttura, offset, byte opachi — è esattamente quello dei file
di partenza. La dimensione è quella utile: Dolphin alloca il file NAND molto più
grande e riempie il resto di zeri.

## Perché i checksum sono ricalcolati fuori da Rust

`anonymize.py` firma i file con `zlib.crc32` e con un CRC-16/CCITT scritto in
Python. I test Rust verificano i checksum con la **propria** implementazione: se
i due valori coincidono su 160 KB e 127 KB di dati reali, l'algoritmo e la
finestra sono quelli giusti. Se le fixture fossero firmate dal codice sotto
test, il confronto non proverebbe nulla.

## Rigenerare le fixture

```bash
python anonymize.py rksys  <percorso>/rksys.dat  rksys.dat
python anonymize.py rfldb  <percorso>/RFL_DB.dat RFL_DB.dat
```

Su Windows i file di partenza stanno in
`%USERPROFILE%\Documents\User\Wii\...` oppure nella cartella User di Dolphin
configurata nel launcher.

Dopo la rigenerazione le aspettative numeriche dei test (numero di licenze, di
amici, di Mii) vanno riallineate al nuovo file.
