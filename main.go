package main

import (
	"flag"
	"os"

	log "github.com/sirupsen/logrus"
	"github.com/suse-edge/nm-configurator/pkg/config"
	"github.com/suse-edge/nm-configurator/pkg/configurator"
)

const systemConnectionsDir = "/etc/NetworkManager/system-connections"

func init() {
	log.SetFormatter(&log.TextFormatter{
		FullTimestamp:    true,
		QuoteEmptyFields: true,
	})
	log.SetOutput(os.Stdout)
}

func main() {
	var (
		configDir       string
		hostsConfigFile string
		verbose         bool
	)

	flag.StringVar(&configDir, "config-dir", "config", "directory storing host mapping ('host_config.yaml') and *.nmconnection files per host")
	flag.StringVar(&hostsConfigFile, "hosts-config-file", "host_config.yaml", "name of the hosts config file mapping interfaces to the respective MAC addresses")
	flag.BoolVar(&verbose, "verbose", false, "enables DEBUG log level")
	flag.Parse()

	if verbose {
		log.SetLevel(log.DebugLevel)
	}

	if err := os.MkdirAll(systemConnectionsDir, 0755); err != nil {
		log.Fatalf("failed to create \"system-connections\" dir: %s", err)
	}

	conf, err := config.Load(configDir, hostsConfigFile, systemConnectionsDir)
	if err != nil {
		log.Fatalf("failed to load static host configuration: %s", err)
	}

	networkInterfaces, err := configurator.GetNetworkInterfaces()
	if err != nil {
		log.Fatalf("failed to load system network interfaces: %s", err)
	}

	c := configurator.New(conf, networkInterfaces)
	if err = c.Run(); err != nil {
		log.Fatalf("failed to configure network manager: %s", err)
	}
	log.Info("successfully configured network manager")
}
