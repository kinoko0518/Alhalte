import 'noph-ui/defaultTheme';
import { mount } from 'svelte';
import App from './components/App.svelte';
import { applyThemeToDocument } from './utils/theme.js';

applyThemeToDocument();

const app = mount(App, {
  target: document.getElementById('app')!,
  props: { isAdmin: true },
});

export default app;
