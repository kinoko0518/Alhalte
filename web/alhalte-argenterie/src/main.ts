import 'noph-ui/defaultTheme';
import { mount } from 'svelte';
import App from './components/App.svelte';
import { completeGoogleSignIn } from './utils/account.js';
import { applyThemeToDocument } from './utils/theme.js';

applyThemeToDocument();

// Complete a pending Google OAuth redirect before mounting so the room URL
// (including its key fragment) and the new session are available to App.
try {
  await completeGoogleSignIn();
} catch (err) {
  if (typeof process === 'undefined' || process.env?.NODE_ENV !== 'test') {
    console.error('Failed to complete Google sign-in:', err);
  }
}

// Admin is derived from the room role returned by maitre-d (Issue #156).
const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
