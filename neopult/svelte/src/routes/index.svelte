<script lang="ts">
    import { socketConnectionStore, reconnect } from '$lib/neopult';
    import Login from '$components/Login.svelte';
    import App from '$components/App.svelte';
</script>

<svelte:window
    on:focus={() => {
        if ($socketConnectionStore.tryingReconnect) {
            reconnect();
        }
    }}
/>

<!-- <pre>{JSON.stringify($socketConnectionStore, null, 2)}</pre> -->

{#if !$socketConnectionStore.connected && $socketConnectionStore.initialConnect}
    <Login />
{:else}
    <App />
{/if}
