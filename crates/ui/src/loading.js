await new Promise(resolve => {
    let firstFrame;
    let secondFrame;
    let timer;
    let finished = false;
    const finish = () => {
        if (finished) return;
        finished = true;
        cancelAnimationFrame(firstFrame);
        cancelAnimationFrame(secondFrame);
        clearTimeout(timer);
        resolve();
    };
    timer = setTimeout(finish, 100);
    firstFrame = requestAnimationFrame(() => {
        secondFrame = requestAnimationFrame(finish);
    });
});
dioxus.send(true);
