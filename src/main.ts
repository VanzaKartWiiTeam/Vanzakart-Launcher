/** Punto d'ingresso del frontend. */
import { mount } from 'svelte';

import './lib/styles/global.css';
import { effects } from './lib/stores/effects.svelte';
import { i18n } from './lib/stores/i18n.svelte';
import App from './routes/App.svelte';

// L'`index.html` dichiara `lang="it"`: qui vince la lingua scelta davvero.
i18n.apply();

// Effetti pieni o ridotti: la scelta della volta scorsa vale da subito, prima
// che si disegni il primo fotogramma (§D-082).
effects.apply();

const target = document.getElementById('app');
if (!target) throw new Error('elemento #app mancante');

export default mount(App, { target });
