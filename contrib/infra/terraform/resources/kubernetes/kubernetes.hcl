locals {
  kubernetes_vars = read_terragrunt_config(find_in_parent_folders("cluster.hcl"))
}

generate "kubernetes.tf" {
  path      = "kubernetes.tf"
  if_exists = "overwrite_terragrunt"

  contents  = <<EOF
  provider "kubernetes" {
    host                   = "${local.kubernetes_vars.locals.kube_host}"
    token                  = "${local.kubernetes_vars.locals.kube_token}"
  }

  provider "helm" {
    kubernetes = {
      host                   = "${local.kubernetes_vars.locals.kube_host}"
      token                  = "${local.kubernetes_vars.locals.kube_token}"
    }
  }

  provider "tailscale" {}
EOF
}

generate "argocd.tf" {
  path = "argocd.tf"
  if_exists = "overwrite_terragrunt"
  disable = local.kubernetes_vars.locals.argocd_token == ""

  contents = <<EOF
  provider "argocd" {
    grpc_web = true
    server_addr = "${local.kubernetes_vars.locals.argocd_url}"
    auth_token = "${local.kubernetes_vars.locals.argocd_token}"
  }
EOF

}