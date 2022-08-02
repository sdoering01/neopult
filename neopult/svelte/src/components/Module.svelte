<script lang="ts">
    import { type Module, callAction } from '$lib/neopult';
    import Button from '$components/Button.svelte';

    export let pluginInstanceName: string;
    export let module: Module;

    let statusClasses = '';
    $: {
        // TODO: Replace with an additional variable that is independent from the status text
        if (module.status === 'waiting') {
            statusClasses = 'bg-yellow-500';
        } else if (module.status === 'active') {
            statusClasses = 'bg-green-500';
        } else {
            statusClasses = 'bg-slate-700';
        }
    }
</script>

<div
    class="relative flex flex-col p-4 pl-7 rounded-lg bg-slate-900 text-white break-words w-full max-w-full shadow-sm"
>
    <span
        class="absolute left-3 top-0 bottom-0 my-4 w-1.5 rounded-full transition {statusClasses}"
    />
    <h3 class="text-2xl">
        {module.displayName}
        {#if module.status}
            <span class="leading-8 align-bottom text-sm rounded-full bg-slate-700 py-0.5 px-1.5"
                >{module.status}</span
            >
        {/if}
    </h3>
    <div class="flex items-center flex-wrap mt-2 gap-2">
        {#each Object.values(module.actions) as action (action.name)}
            <Button
                responsive
                active={action.active}
                on:click={() => callAction(pluginInstanceName, module.name, action.name)}
                >{action.displayName}</Button
            >
        {/each}
    </div>
    {#if module.message}
        <!-- Plugin authors have to make sure that messages are escaped properly -->
        <div class="message mt-3 px-3 py-2 bg-slate-700 rounded-md text-white">
            {@html module.message}
        </div>
    {/if}
</div>

<style>
    .message :global(a) {
        @apply underline text-slate-300;
    }
</style>
