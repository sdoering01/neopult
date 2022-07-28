<script lang="ts">
    import {
        socketConnectionStore,
        neopultStore,
        reconnect,
        logout,
    } from '$lib/neopult';
    import Module from '$components/Module.svelte';
    import Button from '$components/Button.svelte';
</script>

<Button on:click={logout}>Logout</Button>
<div class="relative max-w-5xl mx-auto">
    {#if !$socketConnectionStore.connected}
        <div class="absolute inset-0 z-30  bg-black bg-opacity-75">
            <div class="bg-slate-900 text-white">
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
    <div class="p-2 flex flex-col items-start gap-2">
        {#each Object.values($neopultStore.pluginInstances) as pluginInstance (pluginInstance.name)}
            {#each Object.values(pluginInstance.modules) as module (module.name)}
                <Module pluginInstanceName={pluginInstance.name} {module} />
            {/each}
        {/each}
    </div>
</div>
