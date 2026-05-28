import type { NextConfig } from "next";

const engineApiUrl = process.env.ENGINE_API_URL ?? "http://127.0.0.1:8080";

const nextConfig: NextConfig = {
  output: "standalone",
  async rewrites() {
    return [
      {
        source: "/api/v1/:path*",
        destination: `${engineApiUrl}/api/v1/:path*`,
      },
    ];
  },
};

export default nextConfig;
