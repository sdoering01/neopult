<script lang="ts">
    import { socketConnectionStore, neopultStore } from '$lib/neopult';
    import Module from '$components/Module.svelte';
</script>

<pre>{JSON.stringify($socketConnectionStore, null, 2)}</pre>

{#if $socketConnectionStore.connected}
    <div class="max-w-5xl mx-auto p-2 flex flex-col items-start gap-2">
        {#each Object.values($neopultStore.pluginInstances) as pluginInstance (pluginInstance.name)}
            {#each Object.values(pluginInstance.modules) as module (module.name)}
                <Module pluginInstanceName={pluginInstance.name} {module} />
            {/each}
        {/each}
    </div>
{/if}
