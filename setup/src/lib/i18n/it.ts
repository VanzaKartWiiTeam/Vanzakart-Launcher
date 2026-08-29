/**
 * Dizionario italiano dell'installer: è il riferimento.
 *
 * Le chiavi definite qui sono quelle che `en.ts` deve avere, una per una —
 * il tipo lo impone. I segnaposto si scrivono `{nome}`.
 */
export const it = {
  // ── Comune ──────────────────────────────────────────────────────────────
  'common.cancel': 'Annulla',
  'common.close': 'Chiudi',
  'common.retry': 'Riprova',
  'common.browse': 'Sfoglia',
  'common.dash': '—',

  // ── Guscio ──────────────────────────────────────────────────────────────
  'app.badge.install': 'Setup · v{version}',
  'app.badge.uninstall': 'Disinstallazione · v{version}',
  'app.preparing': 'Preparazione…',
  'app.bootFailed': "Non riesco ad avviare l'installer.",
  'titlebar.minimize': 'Riduci a icona',
  'titlebar.close': 'Chiudi',
  'titlebar.busy': 'Attendi la fine dell’operazione in corso',
  'titlebar.language': 'Lingua',

  // ── Passi ───────────────────────────────────────────────────────────────
  'rail.nav': "Passi dell'installazione",
  'step.welcome': 'Benvenuto',
  'step.welcome.hint': 'Cosa verrà installato',
  'step.folder': 'Cartella',
  'step.folder.hint': 'Dove e con quali scorciatoie',
  'step.checks': 'Verifiche',
  'step.checks.hint': 'Spazio, permessi, launcher',
  'step.running': 'Installazione',
  'step.running.hint': 'Download ed estrazione',
  'step.done': 'Fine',
  'step.done.hint': 'Riepilogo e avvio',

  // ── Benvenuto ───────────────────────────────────────────────────────────
  'welcome.eyebrow': 'Installazione guidata',
  'welcome.subtitle':
    'Scarica e installa il launcher della modpack su {platform}. Da lì si installano la modpack, il music pack e gli addon, e si avvia il gioco.',
  'welcome.releaseTitle': 'Versione da installare',
  'welcome.published': 'Pubblicata il {date}',
  'welcome.package': 'Pacchetto',
  'welcome.size': 'Dimensione',
  'welcome.sizeUnknown': 'da leggere',
  'welcome.verify': 'Verifica',
  'welcome.verify.sha': 'impronta SHA-256',
  'welcome.verify.none': 'non dichiarata',
  'welcome.releaseFailed': "Non riesco a leggere l'elenco dei pacchetti dal server.",
  'welcome.unknownCause': 'Causa sconosciuta.',
  'welcome.openDownloads': 'Apri la pagina dei download',
  'welcome.existing': 'Installazione già presente',
  'welcome.existing.version': 'Versione {version}',
  'welcome.existing.unknownVersion': 'Versione sconosciuta',
  'welcome.existing.managed':
    'Verrà aggiornata sul posto. Al passo successivo puoi scegliere una reinstallazione pulita.',
  'welcome.existing.foreign':
    'Non è stata installata da questa procedura: si può comunque aggiornare o sostituire.',
  'welcome.legacy': "C'è anche il launcher precedente",
  'welcome.legacy.note':
    'Resta dov’è: il launcher nuovo si installa in una cartella sua e al primo avvio importa le impostazioni di quello vecchio, senza toccarne i file. Quando non ti serve più, disinstallalo con il suo disinstallatore.',
  'welcome.footnote':
    "L'installazione non richiede privilegi di amministratore e non tocca i dati di gioco già presenti.",

  // ── Cartella e scorciatoie ──────────────────────────────────────────────
  'folder.eyebrow': 'Passo 2',
  'folder.title': 'Cartella e scorciatoie',
  'folder.installDir': "Cartella d'installazione",
  'folder.suggestions': 'Proposte:',
  'folder.modes': "C'è già un'installazione qui",
  'folder.mode.update': 'Aggiorna',
  'folder.mode.update.note': 'Sostituisce i file del programma e lascia il resto dov’è.',
  'folder.mode.clean': 'Reinstallazione pulita',
  'folder.mode.clean.note':
    'Svuota la cartella prima di installare. Impostazioni, modpack e salvataggi stanno altrove e non vengono toccati.',
  'folder.shortcuts': 'Scorciatoie',
  'folder.shortcut.desktop': 'Sul desktop',
  'folder.shortcut.startMenu': 'Nel menu applicazioni',
  'folder.shortcut.uninstallEntry': 'Voce per disinstallare',
  'folder.shortcut.quickLaunch': 'Nella barra di avvio veloce',
  'folder.shortcut.symlinkBefore': 'Comando',
  'folder.shortcut.symlinkMiddle': 'in',
  'folder.before': 'Prima di procedere',
  'folder.backupData': 'Copia le impostazioni del launcher',
  'folder.backupDir': 'Cartella del backup',
  'folder.backupNote':
    'Vengono copiate le impostazioni e i percorsi configurati. Il token della beta non viene copiato.',

  // ── Verifiche ───────────────────────────────────────────────────────────
  'checks.eyebrow': 'Passo 3',
  'checks.title': 'Verifiche',
  'checks.subtitle': 'Installa VanzaKart Launcher {version} in',
  'checks.requiredSpace': 'Spazio necessario',
  'checks.availableSpace': 'Spazio disponibile',
  'checks.unmeasurable': 'non rilevabile',
  'checks.download': 'Da scaricare',
  'checks.undeclared': 'non dichiarato',
  'checks.writable': 'Cartella scrivibile',
  'checks.writable.yes': 'sì',
  'checks.writable.no': 'no, servono altri permessi',
  'checks.running': 'Launcher in esecuzione',
  'checks.running.yes': 'sì, va chiuso',
  'checks.running.no': 'no',
  'checks.verify': 'Verifica del pacchetto',
  'checks.verify.sha': 'impronta SHA-256',
  'checks.verify.none': 'non dichiarata dal server',
  'checks.checking': 'Controllo in corso…',
  'checks.failed': 'Le verifiche non sono riuscite.',
  'checks.noSpace':
    "Sul disco scelto non c'è abbastanza spazio. Libera spazio o scegli un'altra cartella.",
  'checks.launcherOpen':
    'VanzaKart Launcher è aperto. Chiudilo e ripeti le verifiche: i suoi file non si possono sostituire mentre è in esecuzione.',
  'checks.notWritable':
    'Nella cartella scelta non si può scrivere. Scegline una dentro la tua cartella utente.',
  'checks.readyBefore': 'Tutto pronto: premi',
  'checks.readyAfter': 'per procedere.',

  // ── Avanzamento ─────────────────────────────────────────────────────────
  'progress.installing': 'Installazione in corso',
  'progress.removing': 'Rimozione in corso',
  'progress.starting': 'Avvio',
  'progress.preparing': 'Preparazione…',
  'progress.percent': 'Avanzamento',
  'progress.downloaded': 'Scaricato',
  'progress.speed': 'Velocità',
  'progress.eta': 'Tempo rimanente',
  'progress.log': 'Registro',

  // ── Fine ────────────────────────────────────────────────────────────────
  'done.eyebrow': 'Fatto',
  'done.title': 'VanzaKart Launcher {version} è installato',
  'done.subtitle':
    'Al primo avvio il launcher chiede dove sono Dolphin e la ROM, poi scarica la modpack.',
  'done.folder': 'Cartella',
  'done.size': 'Spazio occupato',
  'done.uninstaller': 'Disinstallatore',
  'done.backup': 'Backup delle impostazioni',
  'done.launchAfter': 'Avvia VanzaKart Launcher alla chiusura',

  // ── Procedura guidata ───────────────────────────────────────────────────
  'wizard.next': 'Avanti',
  'wizard.back': 'Indietro',
  'wizard.install': 'Installa',
  'wizard.finish': 'Fine',
  'wizard.dialog.installDir': "Scegli la cartella d'installazione",
  'wizard.dialog.backupDir': 'Scegli la cartella del backup',
  'wizard.status.rereading': 'Rilettura dal server…',
  'wizard.status.available': 'Versione disponibile: {version}.',
  'wizard.status.checksDone': 'Verifiche completate.',
  'wizard.status.noSpace': 'Spazio insufficiente.',
  'wizard.status.installing': 'Installazione in corso…',
  'wizard.status.installed': 'Installazione completata.',
  'wizard.status.cancelled': 'Installazione annullata.',
  'wizard.status.cancelling': 'Annullamento…',

  // ── Disinstallazione ────────────────────────────────────────────────────
  'uninstall.eyebrow': 'Disinstallazione',
  'uninstall.title': 'Rimuovi VanzaKart Launcher',
  'uninstall.version': 'Versione {version} ·',
  'uninstall.what': 'Cosa rimuovere oltre al programma',
  'uninstall.cache': 'Cache, log e download interrotti',
  'uninstall.cache.note': 'Si rigenerano da soli. Non contengono nulla di tuo.',
  'uninstall.data': 'Impostazioni e dati del launcher',
  'uninstall.data.note':
    'Percorsi di Dolphin, preferenze, Mii importati. Reinstallando dovrai riconfigurare tutto.',
  'uninstall.modpacks': 'Modpack installate in Dolphin',
  'uninstall.modpacks.note': 'Le cartelle VanzaKart e VKBeta dentro Load/Riivolution.',
  'uninstall.modpacks.none': 'Nessuna modpack trovata: non c’è niente da togliere.',
  'uninstall.userData': 'Salvataggi e personalizzazioni della modpack',
  'uninstall.userData.before': 'I dati di gioco in',
  'uninstall.userData.after': ': licenze, tempi, addon locali. Non si recuperano.',
  'uninstall.willRemove': 'Verrà rimosso',
  'uninstall.nothing': 'Niente da rimuovere.',
  'uninstall.unmanaged':
    'Installazione non registrata: verranno tolti la cartella e le scorciatoie nei percorsi noti.',
  'uninstall.run': 'Disinstalla',
  'uninstall.confirm.title': 'Disinstalla VanzaKart Launcher',
  'uninstall.confirm.body':
    'Verranno rimossi anche dati che non si possono recuperare: impostazioni, modpack o salvataggi, secondo quanto hai scelto. Procedo?',
  'uninstall.done.title': 'VanzaKart Launcher è stato rimosso',
  'uninstall.done.summary': '{count} elementi rimossi · {size} liberati',
  'uninstall.done.deferred':
    "La cartella d'installazione viene cancellata alla chiusura di questa finestra: contiene il programma che stai usando adesso.",
  'uninstall.done.thanks':
    'Grazie per aver corso con noi. Puoi reinstallare quando vuoi dalla pagina dei download.',
  'uninstall.missing': 'Nessuna installazione di VanzaKart Launcher trovata su questo computer.'
} as const;

/** Forma che ogni dizionario dell'installer deve avere: quella dell'italiano. */
export type SetupDictionary = Record<keyof typeof it, string>;
