export const parseChannel = (params: URLSearchParams) => {
    let channel = 1;
    const channelParam = params.get('channel');
    if (channelParam !== null) {
        const channelNum = parseInt(channelParam);
        if (!isNaN(channelNum) && channelNum >= 0 && channelNum <= 99) {
            channel = channelNum;
        } else {
            console.error("invalid channel parameter");
        }
    }
    return channel;
};
