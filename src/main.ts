/** Punto d'ingresso del frontend. */
import { mount } from 'svelte';

import './lib/styles/global.css';
import { i18n } from './lib/stores/i18n.svelte';
import App from './routes/App.svelte';

// L'`index.html` dichiara `lang="it"`: qui vince la lingua scelta davvero.
i18n.apply();

const target = document.getElementById('app');
if (!target) throw new Error('elemento #app mancante');

export default mount(App, { target });
