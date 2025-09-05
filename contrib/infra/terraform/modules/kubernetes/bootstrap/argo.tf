resource "kubernetes_namespace" "argocd" {
  metadata {
    name = "argocd"
  }
}

resource "helm_release" "argocd" {
  name       = "argocd"
  repository = "https://argoproj.github.io/argo-helm"
  chart      = "argo-cd"
  version    = var.argocd_version
  namespace  = kubernetes_namespace.argocd.metadata[0].name

  set = [
    {
      name  = "configs.cm.accounts\\.terraform",
      value = "apiKey"
    },
    {
      name  = "configs.rbac.policy\\.csv",
      value = "g\\,terraform\\,role:admin"
      type  = "string"
    },
    {
      name  = "server.service.type"
      value = "ClusterIP"
    },
    {
      name  = "server.ingress.enabled"
      value = "true"
    },
    {
      name  = "server.ingress.ingressClassName"
      value = "tailscale"
    },
    {
      name  = "server.ingress.annotations.tailscale\\.com/expose"
      value = "true"
    },
    {
      name  = "server.ingress.annotations.tailscale\\.com/hostname"
      value = var.argocd_tailscale_hostname
    },
    {
      name  = "server.ingress.hosts[0].host"
      value = var.argocd_tailscale_hostname
    },
    {
      name  = "server.ingress.hosts[0].paths[0].path"
      value = "/"
    },
    {
      name  = "server.ingress.hosts[0].paths[0].pathType"
      value = "Prefix"
    },
    {
      name  = "server.ingress.tls[0].hosts[0]"
      value = var.argocd_tailscale_hostname
    },
    {
      name  = "configs.params.server\\.insecure"
      value = "true"
    }
  ]

  depends_on = [
    kubernetes_namespace.argocd,
    helm_release.tailscale_operator
  ]
}