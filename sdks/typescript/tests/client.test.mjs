import test from "node:test";
import assert from "node:assert/strict";
import { IdrisiClient, IdrisiError, createMapLibreVectorSource, createMapLibreTransformRequest } from "../dist/index.js";
const dataset = { id:"d1", slug:"bandung", name:"Bandung", description:null, domain:"roads", status:"published", created_at:"2026-01-01", updated_at:"2026-01-01" };
test("normalizes URL and attaches dynamic bearer token", async () => {
  let seen;
  const client = new IdrisiClient({ baseUrl:"https://maps.example.test/api/idrisi///", tokenProvider:()=>"new-token", fetch:async (url, init) => { seen={url:String(url), headers:new Headers(init?.headers)}; return Response.json([dataset]); } });
  assert.deepEqual(await client.listDatasets(), [dataset]);
  assert.equal(seen.url, "https://maps.example.test/api/idrisi/v1/catalog/datasets");
  assert.equal(seen.headers.get("authorization"), "Bearer new-token");
});
test("encodes identifiers and reports HTTP errors", async () => {
  let urlSeen;
  const client = new IdrisiClient({ baseUrl:"http://localhost:8080", fetch:async url => { urlSeen=String(url); return new Response("missing", {status:404,statusText:"Not Found"}); } });
  await assert.rejects(client.getDataset("bad/id"), error => error instanceof IdrisiError && error.status===404 && error.responseBody==="missing");
  assert.ok(urlSeen.endsWith("/v1/catalog/datasets/bad%2Fid"));
});
test("creates vector tile source on Iket public prefix", () => {
  const client = new IdrisiClient({baseUrl:"https://maps.example.test/api/idrisi",fetch:async()=>Response.json({})});
  assert.deepEqual(createMapLibreVectorSource(client,"osm_roads",{minzoom:5,maxzoom:18}), {type:"vector",tiles:["https://maps.example.test/api/idrisi/tiles/osm_roads/{z}/{x}/{y}"],minzoom:5,maxzoom:18});
});
test("rejects empty base URL", () => assert.throws(() => new IdrisiClient({baseUrl:" "}), /baseUrl is required/));
test("MapLibre transform is synchronous and carries current token", () => {
  const client = new IdrisiClient({baseUrl:"https://maps.example.test", token:"initial", fetch:async()=>Response.json({})});
  const transform = createMapLibreTransformRequest(client);
  assert.equal(transform("https://maps.example.test/tile").headers.authorization, "Bearer initial");
  client.setToken("rotated");
  assert.equal(transform("https://maps.example.test/tile").headers.Authorization, "Bearer rotated");
});
