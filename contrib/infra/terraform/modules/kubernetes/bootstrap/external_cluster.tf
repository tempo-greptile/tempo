resource "kubernetes_manifest" "external_cluster_egress" {
  for_each = var.external_clusters

  manifest = {
    "apiVersion" = "v1"
    "kind" = "Service"
    "metadata" = {
      "name" = "external-cluster-${each.key}"
      "namespace" = "tailscale"
      "annotations" = {
        "tailscale.com/tailnet-fqdn" = "${each.value}"
      }
    }
    "spec" = {
      "type" = "ExternalName"
      "ports" = [
        {
          "name" = "https"
          "port" = "443"
          "protocol" = "TCP"
        }
      ]
    }
  }

  wait {
    condition {
      type = "TailscaleProxyReady"
      status = "True"
    }
  }
}

resource "argocd_cluster" "external_cluster" {
  for_each = var.external_clusters

  server = "https://${each.value}"

  config {
    username = "tailscale-auth"
  }

  lifecycle {
    ignore_changes = [ config[0] ]
  }

  depends_on = [
    kubernetes_manifest.external_cluster_egress
  ]
}