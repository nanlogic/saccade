// Copyright 2026 NaN Logic LLC
// SPDX-License-Identifier: Apache-2.0

#ifndef SACCADE_CEF_HOST_SACCADE_BRAND_RESOURCES_H_
#define SACCADE_CEF_HOST_SACCADE_BRAND_RESOURCES_H_

#include <cstddef>
#include <cstdint>
#include <vector>

#include "include/cef_resource_bundle_handler.h"

// Supplies Saccade-owned product strings and the fallback favicon used by
// Chromium-style tabs that do not provide a site favicon (including New Tab).
// All other resources continue to come from the pinned CEF/Chromium packs.
class SaccadeBrandResources final : public CefResourceBundleHandler {
 public:
  SaccadeBrandResources();

  bool GetLocalizedString(int string_id, CefString& string) override;
  bool GetDataResource(int resource_id,
                       void*& data,
                       size_t& data_size) override;
  bool GetDataResourceForScale(int resource_id,
                               ScaleFactor scale_factor,
                               void*& data,
                               size_t& data_size) override;

 private:
  bool IsSaccadeFaviconResource(int resource_id) const;
  bool ReturnFavicon(void*& data, size_t& data_size);

  std::vector<uint8_t> favicon_png_;

  IMPLEMENT_REFCOUNTING(SaccadeBrandResources);
};

#endif  // SACCADE_CEF_HOST_SACCADE_BRAND_RESOURCES_H_
