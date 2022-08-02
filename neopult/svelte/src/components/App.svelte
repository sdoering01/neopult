<script lang="ts">
    import { socketConnectionStore, neopultStore, reconnect, logout, channel } from '$lib/neopult';
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

<div class="max-w-5xl mx-auto">
    <div class="p-2 flex flex-col items-start gap-2">
        <div
            class="relative flex flex-col items-center justify-center gap-2 p-4 rounded-lg bg-slate-900 text-white w-full max-w-full shadow-sm xs:flex-row"
        >
            <h3 class="text-2xl">Neopult Channel {channel}</h3>
            <div class="w-full xs:absolute xs:right-4 xs:w-auto">
                <Button responsive on:click={logout}>Logout</Button>
            </div>
        </div>
        {#each Object.values($neopultStore.pluginInstances) as pluginInstance (pluginInstance.name)}
            {#each Object.values(pluginInstance.modules) as module (module.name)}
                <Module pluginInstanceName={pluginInstance.name} {module} />
            {/each}
        {/each}
    </div>
</div>
