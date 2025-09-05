resource "helm_release" "coredns" {
  name       = "coredns"
  chart      = "coredns"
  repository = "https://coredns.github.io/helm"
  version    = var.coredns_version
  namespace  = "kube-system"


  set = [
    { name = "replicaCount", value = "2", type = "auto" },
    { name = "isClusterService", value = "true" },
    { name = "serviceType", value = "ClusterIP" },
    { name = "service.clusterIP", value = "10.96.0.10" },
    { name = "servers[0].zones[0].zone", value = "." },
    { name = "servers[0].port", value = "53" },

    { name = "servers[0].plugins[0].name", value = "errors" },

    { name = "servers[0].plugins[1].name", value = "health" },
    {
      name  = "servers[0].plugins[1].configBlock",
      value = <<-EOT
        lameduck 5s
      EOT 
    },

    { name = "servers[0].plugins[2].name", value = "ready" },

    { name = "servers[0].plugins[3].name", value = "log" },
    { name = "servers[0].plugins[3].parameters", value = "." },
    {
      name  = "servers[0].plugins[3].configBlock",
      value = <<-EOT
        class error
      EOT
    },

    { name = "servers[0].plugins[4].name", value = "prometheus" },
    { name = "servers[0].plugins[4].parameters", value = ":9153" },

    { name = "servers[0].plugins[5].name", value = "kubernetes" },
    { name = "servers[0].plugins[5].parameters", value = "cluster.local in-addr.arpa ip6.arpa" },
    {
      name  = "servers[0].plugins[5].configBlock",
      value = <<-EOT
        pods insecure
        fallthrough in-addr.arpa ip6.arpa
        ttl 30
      EOT
    },

    { name = "servers[0].plugins[6].name", value = "forward" },
    { name = "servers[0].plugins[6].parameters", value = ". /etc/resolv.conf" },
    { name  = "servers[0].plugins[6].configBlock",
      value = <<-EOT
        max_concurrent 1000
      EOT
    },

    { name = "servers[0].plugins[7].name", value = "cache" },
    { name = "servers[0].plugins[7].parameters", value = "30" },
    { 
      name  = "servers[0].plugins[7].configBlock",
      value = <<-EOT
        disable success cluster.local
        disable denial cluster.local
      EOT 
    },

    { name = "servers[0].plugins[8].name", value = "loop" },
    { name = "servers[0].plugins[9].name", value = "reload" },
    { name = "servers[0].plugins[10].name", value = "loadbalance" },

    { name = "servers[1].zones[0].zone", value = "ts.net" },
    { name = "servers[1].port", value = "53" },
    { name = "servers[1].plugins[0].name", value = "errors" },
    { name = "servers[1].plugins[1].name", value = "cache" },
    { name = "servers[1].plugins[1].parameters", value = "30" },
    { name = "servers[1].plugins[2].name", value = "forward" },
    { name = "servers[1].plugins[2].parameters", value = ". 10.96.0.11" },
  ]
}