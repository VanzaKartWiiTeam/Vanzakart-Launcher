/**
 * English dictionary for the installer.
 *
 * Keys mirror `it.ts` one to one — the type says so, and a missing or extra
 * key is a compile error. Placeholders are written `{name}`.
 */
import type { SetupDictionary } from './it';

export const en: SetupDictionary = {
  // ── Shared ──────────────────────────────────────────────────────────────
  'common.cancel': 'Cancel',
  'common.close': 'Close',
  'common.retry': 'Try again',
  'common.browse': 'Browse',
  'common.dash': '—',

  // ── Shell ───────────────────────────────────────────────────────────────
  'app.badge.install': 'Setup · v{version}',
  'app.badge.uninstall': 'Uninstall · v{version}',
  'app.preparing': 'Getting ready…',
  'app.bootFailed': 'The installer could not start.',
  'titlebar.minimize': 'Minimise',
  'titlebar.close': 'Close',
  'titlebar.busy': 'Wait for the current operation to finish',
  'titlebar.language': 'Language',

  // ── Steps ───────────────────────────────────────────────────────────────
  'rail.nav': 'Installation steps',
  'step.welcome': 'Welcome',
  'step.welcome.hint': 'What will be installed',
  'step.folder': 'Folder',
  'step.folder.hint': 'Where, and which shortcuts',
  'step.checks': 'Checks',
  'step.checks.hint': 'Space, permissions, launcher',
  'step.running': 'Install',
  'step.running.hint': 'Download and extraction',
  'step.done': 'Done',
  'step.done.hint': 'Summary and launch',

  // ── Welcome ─────────────────────────────────────────────────────────────
  'welcome.eyebrow': 'Setup wizard',
  'welcome.subtitle':
    'Download and install the modpack launcher on {platform}. From there you install the modpack, the music pack and the addons, and start the game.',
  'welcome.releaseTitle': 'Version to install',
  'welcome.published': 'Published on {date}',
  'welcome.package': 'Package',
  'welcome.size': 'Size',
  'welcome.sizeUnknown': 'not stated',
  'welcome.verify': 'Verification',
  'welcome.verify.sha': 'SHA-256 checksum',
  'welcome.verify.none': 'not declared',
  'welcome.releaseFailed': 'The package list could not be read from the server.',
  'welcome.unknownCause': 'Cause unknown.',
  'welcome.openDownloads': 'Open the downloads page',
  'welcome.existing': 'Already installed',
  'welcome.existing.version': 'Version {version}',
  'welcome.existing.unknownVersion': 'Unknown version',
  'welcome.existing.managed':
    'It will be updated in place. On the next step you can choose a clean reinstall instead.',
  'welcome.existing.foreign':
    'It was not installed by this wizard: it can still be updated or replaced.',
  'welcome.legacy': 'The previous launcher is here too',
  'welcome.legacy.note':
    'It stays where it is: the new launcher installs into a folder of its own and, the first time it starts, imports the old settings without touching its files. When you no longer need it, remove it with its own uninstaller.',
  'welcome.footnote':
    'Installing needs no administrator rights and leaves the game data already on this computer alone.',

  // ── Folder and shortcuts ────────────────────────────────────────────────
  'folder.eyebrow': 'Step 2',
  'folder.title': 'Folder and shortcuts',
  'folder.installDir': 'Install folder',
  'folder.suggestions': 'Suggestions:',
  'folder.modes': 'There is already an installation here',
  'folder.mode.update': 'Update',
  'folder.mode.update.note': 'Replaces the program files and leaves everything else where it is.',
  'folder.mode.clean': 'Clean reinstall',
  'folder.mode.clean.note':
    'Empties the folder before installing. Settings, modpacks and saves live elsewhere and are not touched.',
  'folder.shortcuts': 'Shortcuts',
  'folder.shortcut.desktop': 'On the desktop',
  'folder.shortcut.startMenu': 'In the applications menu',
  'folder.shortcut.uninstallEntry': 'Uninstall entry',
  'folder.shortcut.quickLaunch': 'In the Quick Launch bar',
  'folder.shortcut.symlinkBefore': 'A',
  'folder.shortcut.symlinkMiddle': 'command in',
  'folder.before': 'Before you continue',
  'folder.backupData': 'Copy the launcher settings',
  'folder.backupDir': 'Backup folder',
  'folder.backupNote': 'Settings and configured paths are copied. The beta token is not.',

  // ── Checks ──────────────────────────────────────────────────────────────
  'checks.eyebrow': 'Step 3',
  'checks.title': 'Checks',
  'checks.subtitle': 'Installs VanzaKart Launcher {version} into',
  'checks.requiredSpace': 'Space needed',
  'checks.availableSpace': 'Space available',
  'checks.unmeasurable': 'cannot be measured',
  'checks.download': 'To download',
  'checks.undeclared': 'not stated',
  'checks.writable': 'Folder writable',
  'checks.writable.yes': 'yes',
  'checks.writable.no': 'no, more permissions needed',
  'checks.running': 'Launcher running',
  'checks.running.yes': 'yes, close it',
  'checks.running.no': 'no',
  'checks.verify': 'Package verification',
  'checks.verify.sha': 'SHA-256 checksum',
  'checks.verify.none': 'not declared by the server',
  'checks.checking': 'Checking…',
  'checks.failed': 'The checks could not be completed.',
  'checks.noSpace':
    'There is not enough room on the chosen drive. Free some space or pick another folder.',
  'checks.launcherOpen':
    'VanzaKart Launcher is open. Close it and run the checks again: its files cannot be replaced while it is running.',
  'checks.notWritable': 'The chosen folder cannot be written to. Pick one inside your user folder.',
  'checks.readyBefore': 'All set: press',
  'checks.readyAfter': 'to continue.',

  // ── Progress ────────────────────────────────────────────────────────────
  'progress.installing': 'Installing',
  'progress.removing': 'Removing',
  'progress.starting': 'Starting',
  'progress.preparing': 'Getting ready…',
  'progress.percent': 'Progress',
  'progress.downloaded': 'Downloaded',
  'progress.speed': 'Speed',
  'progress.eta': 'Time left',
  'progress.log': 'Log',

  // ── Done ────────────────────────────────────────────────────────────────
  'done.eyebrow': 'Done',
  'done.title': 'VanzaKart Launcher {version} is installed',
  'done.subtitle':
    'The first time it starts, the launcher asks where Dolphin and the ROM are, then downloads the modpack.',
  'done.folder': 'Folder',
  'done.size': 'Space used',
  'done.uninstaller': 'Uninstaller',
  'done.backup': 'Settings backup',
  'done.launchAfter': 'Start VanzaKart Launcher when this window closes',

  // ── Wizard ──────────────────────────────────────────────────────────────
  'wizard.next': 'Next',
  'wizard.back': 'Back',
  'wizard.install': 'Install',
  'wizard.finish': 'Finish',
  'wizard.dialog.installDir': 'Choose the install folder',
  'wizard.dialog.backupDir': 'Choose the backup folder',
  'wizard.status.rereading': 'Reading from the server again…',
  'wizard.status.available': 'Version available: {version}.',
  'wizard.status.checksDone': 'Checks complete.',
  'wizard.status.noSpace': 'Not enough space.',
  'wizard.status.installing': 'Installing…',
  'wizard.status.installed': 'Installation complete.',
  'wizard.status.cancelled': 'Installation cancelled.',
  'wizard.status.cancelling': 'Cancelling…',

  // ── Uninstall ───────────────────────────────────────────────────────────
  'uninstall.eyebrow': 'Uninstall',
  'uninstall.title': 'Remove VanzaKart Launcher',
  'uninstall.version': 'Version {version} ·',
  'uninstall.what': 'What to remove besides the program',
  'uninstall.cache': 'Cache, logs and interrupted downloads',
  'uninstall.cache.note': 'They come back on their own. Nothing of yours is in there.',
  'uninstall.data': 'Launcher settings and data',
  'uninstall.data.note':
    'Dolphin paths, preferences, imported Miis. Reinstalling means setting everything up again.',
  'uninstall.modpacks': 'Modpacks installed in Dolphin',
  'uninstall.modpacks.note': 'The VanzaKart and VKBeta folders inside Load/Riivolution.',
  'uninstall.modpacks.none': 'No modpack found: there is nothing to remove.',
  'uninstall.userData': 'Modpack saves and customisations',
  'uninstall.userData.before': 'The game data in',
  'uninstall.userData.after': ': licences, times, local addons. They cannot be recovered.',
  'uninstall.willRemove': 'Will be removed',
  'uninstall.nothing': 'Nothing to remove.',
  'uninstall.unmanaged':
    'Unregistered installation: the folder and the shortcuts in the known places will be removed.',
  'uninstall.run': 'Uninstall',
  'uninstall.confirm.title': 'Uninstall VanzaKart Launcher',
  'uninstall.confirm.body':
    'This will also remove data that cannot be recovered: settings, modpacks or saves, according to what you chose. Continue?',
  'uninstall.done.title': 'VanzaKart Launcher has been removed',
  'uninstall.done.summary': '{count} items removed · {size} freed',
  'uninstall.done.deferred':
    'The install folder is deleted when this window closes: it holds the program you are using right now.',
  'uninstall.done.thanks':
    'Thanks for racing with us. You can reinstall whenever you like from the downloads page.',
  'uninstall.missing': 'No VanzaKart Launcher installation found on this computer.'
};
