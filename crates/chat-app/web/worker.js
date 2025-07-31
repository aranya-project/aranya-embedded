async function addResourcesToCache(resources) {
  const cache = await caches.open('v1');
  await cache.addAll(resources);
}

self.addEventListener('install', (event) => {
  event.waitUntil(
    addResourcesToCache([
      'index.html',
      'Web437_PhoenixVGA_9x16.woff',
      'favicon.png'
    ])
  );
});

self.addEventListener('fetch', async (event) => {
  const response = await caches.match(event.request);
  if (response) {
    event.respondWith(response);
  } else {
    event.respondWith(fetch(event.request));
  }
});
