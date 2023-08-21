package configurator

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	log "github.com/sirupsen/logrus"
	"github.com/suse-edge/nm-configurator/pkg/config"
	"gopkg.in/ini.v1"
)

const connectionFileExt = ".nmconnection"

type Configurator struct {
	config            *config.Config
	networkInterfaces NetworkInterfaces
}

func New(config *config.Config, interfaces NetworkInterfaces) *Configurator {
	return &Configurator{
		config:            config,
		networkInterfaces: interfaces,
	}
}

func (c *Configurator) Run() error {
	host, err := c.identifyHost()
	if err != nil {
		return fmt.Errorf("identifying host: %w", err)
	}
	log.Infof("successfully identified host: %s", host.Name)

	if err = c.copyConnectionFiles(host); err != nil {
		return fmt.Errorf("copying files: %w", err)
	}

	return nil
}

// Identify the preconfigured static host by matching the MAC address of at least one of the local network interfaces.
func (c *Configurator) identifyHost() (*config.Host, error) {
	for _, host := range c.config.Hosts {
		for _, i := range host.Interfaces {
			if _, ok := c.networkInterfaces[i.MACAddress]; ok {
				return host, nil
			}
		}
	}

	return nil, fmt.Errorf("none of the preconfigured hosts match local NICs")
}

// Copy all *.nmconnection files from the preconfigured host dir to the
// appropriate NetworkManager dir (default "/etc/NetworkManager/system-connections").
func (c *Configurator) copyConnectionFiles(host *config.Host) error {
	hostConfigDir := filepath.Join(c.config.SourceDir, host.Name)
	dirEntries, err := os.ReadDir(hostConfigDir)
	if err != nil {
		return err
	}

	var errs []error

	for _, entry := range dirEntries {
		name := entry.Name()
		if entry.IsDir() {
			log.Warnf("ignoring unexpected directory: %s", name)
			continue
		}

		if filepath.Ext(name) != connectionFileExt {
			log.Warnf("ignoring unexpected file: %s", name)
			continue
		}

		source := filepath.Join(hostConfigDir, name)
		file, err := ini.Load(source)
		if err != nil {
			errs = append(errs, fmt.Errorf("loading file %s: %w", source, err))
			continue
		}

		destination := filepath.Join(c.config.DestinationDir, name)
		filename := strings.TrimSuffix(name, connectionFileExt)

		// Update the name and all references of the host NIC in the settings file if there is a difference from the static config.
		for _, i := range host.Interfaces {
			if i.LogicalName != filename {
				continue
			}

			interfaceName, ok := c.networkInterfaces[i.MACAddress]
			if ok && interfaceName != i.LogicalName {
				log.Debugf("using name '%s' for interface with MAC address '%s' instead of the preconfigured '%s'",
					interfaceName, i.MACAddress, i.LogicalName)

				for _, section := range file.Sections() {
					if !section.HasValue(i.LogicalName) {
						continue
					}

					for _, key := range section.Keys() {
						if key.Value() == i.LogicalName {
							key.SetValue(interfaceName)
						}
					}
				}

				destination = fmt.Sprintf("%s/%s%s", c.config.DestinationDir, interfaceName, connectionFileExt)
			}
			break
		}

		log.Infof("storing file %s...", destination)
		if err = file.SaveTo(destination); err != nil {
			errs = append(errs, fmt.Errorf("storing file %s: %w", destination, err))
			continue
		}

		// Set the necessary permissions required by NetworkManager.
		if err = os.Chmod(destination, 0600); err != nil {
			errs = append(errs, fmt.Errorf("updating permissions for file %s: %w", destination, err))
		}
	}

	return errors.Join(errs...)
}
