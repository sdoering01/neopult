<script lang="ts">
    export let active = false;
    export let disabled = false;
    export let responsive = false;

    let extraClasses: string;
    $: {
        if (active) {
            extraClasses = 'bg-slate-400 focus-visible:outline-slate-400';
        } else {
            extraClasses = 'bg-slate-200 focus-visible:outline-slate-200';
        }

        if (responsive) {
            extraClasses += ' w-full xs:w-auto';
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
    class="relative outline-none px-2 py-1 rounded-md text-black transition {extraClasses}"
>
    <slot />
</button>
