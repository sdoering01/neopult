<script lang="ts">
    import { socketConnectionStore, connect, reconnect, SocketError, channel } from '$lib/neopult';
    import Button from '$components/Button.svelte';

    const handleFormSubmit = () => {
        connect(passwordInputEl.value, rememberPasswordCheckboxEl.checked);
        passwordInputEl.value = '';
    };

    let passwordInputEl: HTMLInputElement;
    let rememberPasswordCheckboxEl: HTMLInputElement;

    let formDisabled: boolean;
    $: formDisabled = $socketConnectionStore.connecting || $socketConnectionStore.tryingReconnect;
</script>

<div class="fixed inset-0 flex items-center justify-center">
    <div class="bg-slate-900 p-4 rounded-xl text-white">
        <h1 class="text-4xl font-semibold text-center mb-2">Neopult</h1>
        <h3 class="text-2xl text-center mb-6">Channel {channel}</h3>

        {#if $socketConnectionStore.error !== null || (!$socketConnectionStore.connecting && $socketConnectionStore.tryingReconnect)}
            <div
                class="flex flex-col gap-1 items-center bg-red-400 text-black text-center mb-4 p-2 rounded-md"
            >
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

        <form on:submit|preventDefault={handleFormSubmit} class="flex flex-col gap-2">
            <input
                bind:this={passwordInputEl}
                placeholder="Password"
                type="password"
                disabled={formDisabled}
                class="flex-1 px-2 py-1 mr-1 rounded-md text-black w-full"
            />
            <div class="flex items-center justify-between">
                <label for="remember-password-checkbox">Remember password</label>
                <input
                    id="remember-password-checkbox"
                    class="w-4 h-4 accent-slate-400"
                    type="checkbox"
                    disabled={formDisabled}
                    bind:this={rememberPasswordCheckboxEl}
                />
            </div>

            <hr class="my-1 border-slate-500" />

            <Button active disabled={formDisabled}>
                {#if $socketConnectionStore.connecting}
                    <div class="absolute left-2 inset-y-0 flex items-center justify-center">
                        <span
                            class="w-4 h-4 inline-block rounded-full border-2 border-y-black mr-2 animate-spin"
                        />
                    </div>
                {/if}Login
            </Button>
        </form>
    </div>
</div>
