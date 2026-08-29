/** Punto d'ingresso dell'installer. */
import { mount } from 'svelte';

import '$lib/styles/global.css';
import { i18n } from '$setup/lib/i18n/store.svelte';
import App from './routes/App.svelte';

// L'`index.html` dichiara una lingua di comodo: qui vince quella scelta.
i18n.apply();

const target = document.getElementById('app');
if (!target) throw new Error('elemento #app mancante');

export default mount(App, { target });
