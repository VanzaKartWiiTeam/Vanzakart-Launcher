/** Punto d'ingresso dell'installer. */
import { mount } from 'svelte';

import '$lib/styles/global.css';
import App from './routes/App.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('elemento #app mancante');

export default mount(App, { target });
