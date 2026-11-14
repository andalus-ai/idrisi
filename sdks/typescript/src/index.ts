export type FetchLike = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

export interface IdrisiClientOptions {
  /** Public Iket prefix, e.g. https://maps.example.com/api/idrisi */
  baseUrl: string;
  token?: string;
  tokenProvider?: () => string | undefined | Promise<string | undefined>;
  fetch?: FetchLike;
  headers?: HeadersInit;
}

export interface DatasetSummary {
  id: string; slug: string; name: string; description: string | null;
  domain: string; status: string; created_at: string; updated_at: string;
}

export interface TileJson {
  tilejson?: string; name?: string; description?: string; version?: string;
  attribution?: string; scheme?: "xyz" | "tms"; tiles: string[];
  minzoom?: number; maxzoom?: number; bounds?: [number, number, number, number];
  center?: [number, number, number];
  vector_layers: Array<{ id: string; fields?: Record<string, string>; [key: string]: unknown }>;
  [key: string]: unknown;
}

export class IdrisiError extends Error {
  constructor(message: string, readonly status: number, readonly url: string, readonly responseBody: string) {
    super(message); this.name = "IdrisiError";
  }
}

export class IdrisiClient {
  readonly baseUrl: string;
  private token?: string;
  private readonly tokenProvider?: IdrisiClientOptions["tokenProvider"];
  private readonly fetcher: FetchLike;
  private readonly headers: Headers;

  constructor(options: IdrisiClientOptions) {
    const baseUrl = options.baseUrl.trim().replace(/\/+$/, "");
    if (!baseUrl) throw new TypeError("Idrisi baseUrl is required");
    this.baseUrl = baseUrl;
    this.token = options.token;
    this.tokenProvider = options.tokenProvider;
    this.fetcher = options.fetch ?? globalThis.fetch;
    if (!this.fetcher) throw new TypeError("A Fetch implementation is required");
    this.headers = new Headers(options.headers);
    this.headers.set("Accept", "application/json");
  }

  listDatasets(signal?: AbortSignal): Promise<DatasetSummary[]> {
    return this.request<DatasetSummary[]>("/v1/catalog/datasets", signal);
  }
  getDataset(id: string, signal?: AbortSignal): Promise<DatasetSummary> {
    if (!id.trim()) throw new TypeError("datasetId is required");
    return this.request<DatasetSummary>(`/v1/catalog/datasets/${encodeURIComponent(id)}`, signal);
  }
  getTileJson(source: string, signal?: AbortSignal): Promise<TileJson> {
    if (!source.trim()) throw new TypeError("sourceId is required");
    return this.request<TileJson>(`/tiles/${encodeURIComponent(source)}`, signal);
  }
  tileUrl(source: string): string {
    if (!source.trim()) throw new TypeError("sourceId is required");
    return `${this.baseUrl}/tiles/${encodeURIComponent(source)}/{z}/{x}/{y}`;
  }
  setToken(token: string | undefined): void { this.token = token; }

  getMapLibreRequestHeaders(): Headers {
    const headers = new Headers(this.headers);
    if (this.token) headers.set("Authorization", `Bearer ${this.token}`);
    return headers;
  }

  async getRequestHeaders(): Promise<Headers> {
    const headers = new Headers(this.headers);
    const current = await this.tokenProvider?.();
    const token = current ?? this.token;
    if (token) headers.set("Authorization", `Bearer ${token}`);
    return headers;
  }
  private async request<T>(path: string, signal?: AbortSignal): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const response = await this.fetcher(url, { method: "GET", headers: await this.getRequestHeaders(), signal });
    if (!response.ok) {
      throw new IdrisiError(`Idrisi request failed: ${response.status} ${response.statusText}`, response.status, url, await response.text());
    }
    return response.json() as Promise<T>;
  }
}

export interface MapLibreVectorSource {
  type: "vector"; tiles: string[]; minzoom?: number; maxzoom?: number; attribution?: string;
}

/** Build a MapLibre source through Iket; never expose Martin internal URLs. */
export function createMapLibreVectorSource(client: IdrisiClient, sourceId: string, metadata: Pick<TileJson, "minzoom" | "maxzoom" | "attribution"> = {}): MapLibreVectorSource {
  return { type: "vector", tiles: [client.tileUrl(sourceId)],
    ...(metadata.minzoom === undefined ? {} : { minzoom: metadata.minzoom }),
    ...(metadata.maxzoom === undefined ? {} : { maxzoom: metadata.maxzoom }),
    ...(metadata.attribution ? { attribution: metadata.attribution } : {}) };
}

/** Attach current auth headers to MapLibre resource requests. */
export function createMapLibreTransformRequest(client: IdrisiClient) {
  return (url: string) => ({ url, headers: Object.fromEntries(client.getMapLibreRequestHeaders().entries()) });
}