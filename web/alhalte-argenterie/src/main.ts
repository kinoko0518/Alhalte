import 'noph-ui/defaultTheme';
import { mount } from 'svelte';
import App from './components/App.svelte';
import LoginScreen from './components/Login/LoginScreen.svelte';
import { completeGoogleSignIn } from './utils/account.js';
import { applyThemeToDocument } from './utils/theme.js';

applyThemeToDocument();

// `/login` is the local password screen. Do not run the pending-sign-in
// completion there: there is no session yet, and the 401 would delete the
// sessionStorage return URL (including the room key fragment). After a
// successful login the screen navigates back to the app URL; the app mount then
// completes the session and restores the fragment.
const isLoginRoute = window.location.pathname === '/login';

if (!isLoginRoute) {
  try {
    await completeGoogleSignIn();
  } catch (err) {
    if (typeof process === 'undefined' || process.env?.NODE_ENV !== 'test') {
      console.error('Failed to complete sign-in:', err);
    }
  }
}

// Admin is derived from the room role returned by maitre-d (Issue #156).
const app = isLoginRoute
  ? mount(LoginScreen, {
      target: document.getElementById('app')!,
    })
  : mount(App, {
      target: document.getElementById('app')!,
    });

export default app;
