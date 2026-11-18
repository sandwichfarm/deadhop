import { mount } from 'svelte';
import './app.css';
import App from './components/App.svelte';

const target = document.getElementById('app')!;
if (window.top === window.self) {
  mount(App, { target });
} else {
  // Never run inside someone else's frame: a page that embeds this one could
  // trick an operator into clicking sign-in or pasting a key (clickjacking).
  target.textContent = 'This page cannot be shown inside another site.';
}
