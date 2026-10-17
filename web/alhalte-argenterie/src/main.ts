import { mount } from 'svelte';
import App from './components/App.svelte';

const app = mount(App, {
  target: document.getElementById('app')!,
  props: { isAdmin: true },
});

export default app;
