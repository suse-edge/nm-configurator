package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

type Config struct {
	// Configuration directory storing the preconfigured *.nmconnection files per host.
	SourceDir string
	// Destination directory to store the final *.nmconnection files for NetworkManager.
	// Default "/etc/NetworkManager/system-connections".
	DestinationDir string
	Hosts          []*Host `yaml:"host_config"`
}

type Host struct {
	Name       string       `yaml:"hostname"`
	Interfaces []*Interface `yaml:"interfaces"`
}

func (h *Host) String() string {
	return fmt.Sprintf("{Name: %s Interfaces: %+v}", h.Name, h.Interfaces)
}

type Interface struct {
	LogicalName string `yaml:"logical_name"`
	MACAddress  string `yaml:"mac_address"`
}

func (i *Interface) String() string {
	return fmt.Sprintf("{LogicalName: %s MACAddress: %s}", i.LogicalName, i.MACAddress)
}

func Load(sourceDir, configFilename, destinationDir string) (*Config, error) {
	configFile := filepath.Join(sourceDir, configFilename)
	file, err := os.ReadFile(configFile)
	if err != nil {
		return nil, err
	}

	var c Config
	if err = yaml.Unmarshal(file, &c); err != nil {
		return nil, err
	}

	// Ensure lower case formatting.
	for _, host := range c.Hosts {
		for _, i := range host.Interfaces {
			i.MACAddress = strings.ToLower(i.MACAddress)
		}
	}

	c.SourceDir = sourceDir
	c.DestinationDir = destinationDir

	return &c, nil
}
