# Idrisi TypeScript SDK

Provider-neutral TypeScript client for applications consuming Idrisi through the public Iket prefix. The core package has no MapLibre dependency and uses the standard Fetch API.

## Install and test

```sh
cd sdks/typescript
npm install
npm test
```

## Catalog and TileJSON

```ts
import { IdrisiClient } from "@andalus-ai/idrisi";
const idrisi = new IdrisiClient({
  baseUrl: "https://maps.example.com/api/idrisi",
  tokenProvider: async () => authSession.getAccessToken(),
});
const datasets = await idrisi.listDatasets();
const dataset = await idrisi.getDataset(datasets[0].id);
const tileJson = await idrisi.getTileJson("osm_roads");
```

Pass an AbortSignal to catalog and TileJSON methods. Non-2xx responses throw IdrisiError with status, URL, and response body. Avoid logging response bodies indiscriminately.

## MapLibre adapter

```ts
import { IdrisiClient, createMapLibreVectorSource, createMapLibreTransformRequest } from "@andalus-ai/idrisi";
const client = new IdrisiClient({ baseUrl: "https://maps.example.com/api/idrisi", tokenProvider: () => authSession.getAccessToken() });
const metadata = await client.getTileJson("osm_roads");
const source = createMapLibreVectorSource(client, "osm_roads", metadata);
const map = new maplibregl.Map({
  container: "map",
  style: {
    version: 8,
    sources: { roads: source },
    layers: [{ id: "roads", type: "line", source: "roads", "source-layer": "osm_roads",
      paint: { "line-color": "#d8b98a", "line-width": 2 } }]
  },
  transformRequest: createMapLibreTransformRequest(client)
});
```

The source URL is generated from the configured public Iket prefix instead of trusting TileJSON URLs that may expose Martin's private hostname. Confirm Iket maps `/tiles/{source}/{z}/{x}/{y}` to the corresponding Martin endpoint.

## Initial coverage

- listDatasets, getDataset, getTileJson
- Public Iket vector-tile URL template
- Static or refreshable bearer token provider
- AbortSignal support and structured HTTP errors
- Optional MapLibre source and request-transform helpers

Routing, geocoding, raster, offline packages, and mutations are intentionally omitted until stable server contracts exist. Node.js 20+ and TypeScript 5.8+ are used for development; browser clients require Fetch API support. This package is an in-repository preview, not published to npm. License is UNLICENSED pending repository licensing decisions.
