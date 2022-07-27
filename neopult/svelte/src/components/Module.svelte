<script lang="ts">
    import { type Module, callAction } from '$lib/neopult';

    export let pluginInstanceName: string;
    export let module: Module;
</script>

<div>
    <div>{module.displayName} {module.status ? module.status : ''}</div>
    <div>
        {#each Object.values(module.actions) as action (action.name)}
            <button
                class:bg-[#ccc]={action.active}
                on:click={() => callAction(pluginInstanceName, module.name, action.name)}
                >{action.displayName}</button
            >
        {/each}
    </div>
    <!-- Plugin authors have to make sure that messages are escaped properly -->
    {#if module.message}
        <div>{@html module.message}</div>
    {/if}
</div>
