<script lang="ts">
  import { state } from './store.svelte';
  import DeviceScreen from './DeviceScreen.svelte';
  import MixerScreen from './MixerScreen.svelte';
  import ChannelScreen from './ChannelScreen.svelte';

  interface Props {
    screen?: 'left' | 'right' | 'dual';
  }

  let { screen = 'dual' }: Props = $props();
</script>

{#if screen === 'dual' && state.page === 'channel'}
  <!-- The channel page flows as one element across both screens -->
  <main class="dual-app-root">
    <ChannelScreen />
  </main>
{:else if screen === 'dual'}
  <main class="dual-app-root">
    <div class="screen-half left-half">
      {#if state.page === 'mixer'}
        <MixerScreen screen="left" />
      {:else}
        <DeviceScreen screen="left" />
      {/if}
    </div>
    <div class="screen-half right-half">
      {#if state.page === 'mixer'}
        <MixerScreen screen="right" />
      {:else}
        <DeviceScreen screen="right" />
      {/if}
    </div>
  </main>
{:else}
  <main class="app-root">
    {#if state.page === 'mixer'}
      <MixerScreen {screen} />
    {:else if state.page === 'channel'}
      <!-- single-screen build: a 480px window onto the dual-width page -->
      <div class="channel-window" style="transform: translateX({screen === 'right' ? -480 : 0}px);">
        <ChannelScreen />
      </div>
    {:else}
      <DeviceScreen {screen} />
    {/if}
  </main>
{/if}

<style>
  :global(*) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(html, body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 272px;
    overflow: hidden;
    background: #141416;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
    -webkit-font-smoothing: antialiased;
  }

  .dual-app-root {
    display: flex;
    flex-direction: row;
    width: 960px;
    height: 272px;
    overflow: hidden;
    position: relative;
  }

  .screen-half {
    width: 480px;
    height: 272px;
    flex: 0 0 480px;
    overflow: hidden;
    position: relative;
  }

  .channel-window {
    width: 960px;
    height: 272px;
  }

  .app-root {
    width: 480px;
    height: 272px;
    overflow: hidden;
    position: relative;
  }
</style>
