resource "kubernetes_namespace" "onepassword" {
  metadata {
    name = "onepassword"
  }
}

resource "helm_release" "onepassword" {
  name       = "connect"
  repository = "https://1password.github.io/connect-helm-charts/"
  chart      = "connect"
  version    = var.onepassword_connect_version
  namespace  = kubernetes_namespace.onepassword.metadata[0].name

  values = [yamlencode({
    "connect" = {
        "credentials" = var.onepassword_credentials
    }
    "operator" = {
        "create" = true
        "token" = {
            "value" = var.onepassword_secret_token
        }
    }
  })]
}