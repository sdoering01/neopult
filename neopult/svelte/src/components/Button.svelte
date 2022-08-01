<script lang="ts">
    export let active = false;
    export let disabled = false;
    
    let extraClasses: string;
    $: {
        if (active) {
            extraClasses = 'bg-slate-400 focus-visible:outline-slate-400';
        } else {
            extraClasses = 'bg-slate-200 focus-visible:outline-slate-200';
        }

        if (disabled) {
            extraClasses += ' text-neutral-600';
        }
    }
</script>

<button
    on:click
    {disabled}
    class:cursor-not-allowed={disabled}
    class="w-full outline-none px-2 py-1 rounded-md text-black transition xs:w-auto {extraClasses}"
>
    <slot />
</button>
