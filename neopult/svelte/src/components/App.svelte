<script lang="ts">
    import { socketConnectionStore, neopultStore, reconnect, logout } from '$lib/neopult';
    import Module from '$components/Module.svelte';
    import Button from '$components/Button.svelte';
</script>

{#if !$socketConnectionStore.connected}
    <div class="flex items-center justify-center fixed top-0 left-0 w-full h-full z-30 bg-black bg-opacity-75">
        <div class="flex flex-col items-center gap-4 bg-slate-900 text-white p-4 rounded-lg">
            {#if $socketConnectionStore.connecting}
                Reconnecting
            {:else}
                Connection lost <Button on:click={reconnect}
                    >Reconnect (retrying in {Math.ceil(
                        $socketConnectionStore.reconnectInMs / 1000
                    )})</Button
                >
            {/if}
        </div>
    </div>
{/if}

<Button on:click={logout}>Logout</Button>
<div class="max-w-5xl mx-auto">
    <div class="p-2 flex flex-col items-start gap-2">
        {#each Object.values($neopultStore.pluginInstances) as pluginInstance (pluginInstance.name)}
            {#each Object.values(pluginInstance.modules) as module (module.name)}
                <Module pluginInstanceName={pluginInstance.name} {module} />
            {/each}
        {/each}
    </div>
</div>
