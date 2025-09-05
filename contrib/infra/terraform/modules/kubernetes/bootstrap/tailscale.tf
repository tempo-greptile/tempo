resource "kubernetes_namespace" "tailscale" {
  metadata {
    name = "tailscale"
  }
}

resource "tailscale_oauth_client" "kubernetes_operator" {
  description = "OAuth client for Kubernetes operator on ${var.cluster_name}"

  tags = [
    "tag:k8s-operator"
  ]

  scopes = ["devices:core", "auth_keys"]
}

resource "helm_release" "tailscale_operator" {
  name       = "tailscale-operator"
  chart      = "tailscale-operator"
  repository = "https://pkgs.tailscale.com/helmcharts"
  version    = var.tailscale_operator_version
  namespace  = kubernetes_namespace.tailscale.metadata[0].name

  set = [
    {
      name  = "oauth.clientId"
      value = tailscale_oauth_client.kubernetes_operator.id
    },
    {
      name  = "oauth.clientSecret"
      value = tailscale_oauth_client.kubernetes_operator.key
    },
    {
      name  = "apiServerProxyConfig.mode"
      value = "true",
      type  = "string"
    },
    {
      name  = "operatorConfig.hostname"
      value = "${var.cluster_name}-ts-operator"
    }
  ]

  depends_on = [
    kubernetes_namespace.tailscale,
    tailscale_oauth_client.kubernetes_operator
  ]
}

resource "kubernetes_manifest" "tailscale_dns_config" {
  manifest = {
    "apiVersion" = "tailscale.com/v1alpha1"
    "kind" = "DNSConfig"
    "metadata" = {
      "name" = "ts-dns"
    }
    "spec" = {
      "nameserver" = {
        "service" = {
          "clusterIP" = "10.96.0.11"
        }
      }
    }
  }

  depends_on = [
    helm_release.tailscale_operator
  ]
}