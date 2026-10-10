import { mount } from 'svelte';
import App from './App.svelte';
import { initEncdrBridge } from './store.svelte';

initEncdrBridge();

const target = document.getElementById('app');
if (target) {
  mount(App, {
    target,
    props: {
      screen: 'right',
    },
  });
}
