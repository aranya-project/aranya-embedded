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

async function cacheFirst(request) {
  const response = await caches.match(request);
  if (response) {
    return response;
  }
  return fetch(request);
}

self.addEventListener('fetch', (event) => {
  event.respondWith(cacheFirst(event.request));
});
