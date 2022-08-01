<script lang="ts">
    import { socketConnectionStore, connect, reconnect, SocketError } from '$lib/neopult';
    import Button from '$components/Button.svelte';

    let passwordInputEl: HTMLInputElement;

    const handleFormSubmit = () => {
        connect(passwordInputEl.value);
        passwordInputEl.value = '';
    };
</script>

<div class="fixed inset-0 flex items-center justify-center">
    <div class="bg-slate-900 p-4 rounded-xl text-white">
        <h1 class="text-4xl font-semibold text-center mb-6">Neopult</h1>

        {#if $socketConnectionStore.error || (!$socketConnectionStore.connecting && $socketConnectionStore.tryingReconnect)}
            <div class="flex flex-col gap-1 items-center bg-red-400 text-black text-center mb-2 p-2 rounded-md">
                {#if $socketConnectionStore.error === SocketError.PASSWORD_INCORRECT}
                    Password incorrect
                {:else if $socketConnectionStore.error === SocketError.STORED_PASSWORD_INCORRECT}
                    Stored password incorrect
                {:else if $socketConnectionStore.error === SocketError.AUTH_TIMEOUT}
                    Socket authentication timed out
                {:else if !$socketConnectionStore.connecting && $socketConnectionStore.tryingReconnect}
                    Connection failed <Button on:click={reconnect}
                        >Connect (retrying in {Math.ceil(
                            $socketConnectionStore.reconnectInMs / 1000
                        )})</Button
                    >
                {:else}
                    Unhandled connection error
                {/if}
            </div>

        {/if}

        <form on:submit|preventDefault={handleFormSubmit} class="flex items-center">
            <input
                bind:this={passwordInputEl}
                placeholder="Password"
                type="password"
                disabled={$socketConnectionStore.connecting ||
                    $socketConnectionStore.tryingReconnect}
                class="flex-1 px-2 py-1 mr-1 rounded-md text-black"
            />
            <Button
                disabled={$socketConnectionStore.connecting ||
                    $socketConnectionStore.tryingReconnect}
            >
                <div class="flex items-center">
                    {#if $socketConnectionStore.connecting}
                        <span
                            class="w-4 h-4 inline-block rounded-full border-2 border-y-black mr-2 animate-spin"
                        />{/if}Login
                </div>
            </Button>
        </form>
    </div>
</div>
