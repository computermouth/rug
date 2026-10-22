// Favicons are guessed at /favicon.ico and often don't exist. Images with a
// data-fallback swap to it; the rest are hidden. Error events don't bubble,
// so this listens during capture, and runs from <head> so it catches them all.
document.addEventListener('error', function (e) {
  var img = e.target;
  if (!(img instanceof HTMLImageElement)) return;
  if (img.dataset.fallback) {
    img.src = img.dataset.fallback;
    delete img.dataset.fallback;
  } else {
    img.style.visibility = 'hidden';
  }
}, true);
