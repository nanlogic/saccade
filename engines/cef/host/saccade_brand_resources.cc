// Copyright 2026 NaN Logic LLC
// SPDX-License-Identifier: Apache-2.0

#include "tests/cefsimple/saccade_brand_resources.h"

#include <fstream>
#include <iterator>
#include <string>

#include "include/cef_id_mappers.h"
#include "include/cef_path_util.h"

namespace {

std::vector<uint8_t> ReadFile(std::string path) {
  if (!path.empty() && path.back() != '/') {
    path.push_back('/');
  }
  path.append("Saccade-tab.png");

  std::ifstream input(path, std::ios::binary);
  if (!input) {
    return {};
  }
  return std::vector<uint8_t>(std::istreambuf_iterator<char>(input),
                              std::istreambuf_iterator<char>());
}

std::vector<uint8_t> ReadFavicon() {
  CefString resources_path;
  if (CefGetPath(PK_DIR_RESOURCES, resources_path)) {
    auto bytes = ReadFile(resources_path.ToString());
    if (!bytes.empty()) {
      return bytes;
    }
  }

  // CEF may resolve PK_DIR_RESOURCES to the framework resources directory on
  // macOS. The branded asset belongs to the top-level application bundle.
  CefString executable_path;
  if (CefGetPath(PK_DIR_EXE, executable_path)) {
    return ReadFile(executable_path.ToString() + "/../Resources");
  }
  return {};
}

}  // namespace

SaccadeBrandResources::SaccadeBrandResources() : favicon_png_(ReadFavicon()) {}

bool SaccadeBrandResources::GetLocalizedString(int string_id,
                                               CefString& string) {
  CEF_DECLARE_PACK_STRING_ID(IDS_PRODUCT_NAME);
  CEF_DECLARE_PACK_STRING_ID(IDS_SHORT_PRODUCT_NAME);
  CEF_DECLARE_PACK_STRING_ID(IDS_APP_MENU_PRODUCT_NAME);
  CEF_DECLARE_PACK_STRING_ID(IDS_ABOUT_VERSION_COMPANY_NAME);

  if (string_id == IDS_PRODUCT_NAME || string_id == IDS_SHORT_PRODUCT_NAME ||
      string_id == IDS_APP_MENU_PRODUCT_NAME) {
    string = "Saccade";
    return true;
  }
  if (string_id == IDS_ABOUT_VERSION_COMPANY_NAME) {
    string = "NaN Logic LLC";
    return true;
  }
  return false;
}

bool SaccadeBrandResources::GetDataResource(int resource_id,
                                            void*& data,
                                            size_t& data_size) {
  if (!IsSaccadeFaviconResource(resource_id)) {
    return false;
  }
  return ReturnFavicon(data, data_size);
}

bool SaccadeBrandResources::GetDataResourceForScale(
    int resource_id,
    ScaleFactor scale_factor,
    void*& data,
    size_t& data_size) {
  (void)scale_factor;
  if (!IsSaccadeFaviconResource(resource_id)) {
    return false;
  }
  return ReturnFavicon(data, data_size);
}

bool SaccadeBrandResources::IsSaccadeFaviconResource(
    int resource_id) const {
  CEF_DECLARE_PACK_RESOURCE_ID(IDR_DEFAULT_FAVICON);
  CEF_DECLARE_PACK_RESOURCE_ID(IDR_DEFAULT_FAVICON_DARK);
  CEF_DECLARE_PACK_RESOURCE_ID(IDR_DEFAULT_FAVICON_32);
  CEF_DECLARE_PACK_RESOURCE_ID(IDR_DEFAULT_FAVICON_DARK_32);
  CEF_DECLARE_PACK_RESOURCE_ID(IDR_DEFAULT_FAVICON_64);
  CEF_DECLARE_PACK_RESOURCE_ID(IDR_DEFAULT_FAVICON_DARK_64);

  return resource_id == IDR_DEFAULT_FAVICON ||
         resource_id == IDR_DEFAULT_FAVICON_DARK ||
         resource_id == IDR_DEFAULT_FAVICON_32 ||
         resource_id == IDR_DEFAULT_FAVICON_DARK_32 ||
         resource_id == IDR_DEFAULT_FAVICON_64 ||
         resource_id == IDR_DEFAULT_FAVICON_DARK_64;
}

bool SaccadeBrandResources::ReturnFavicon(void*& data, size_t& data_size) {
  if (favicon_png_.empty()) {
    return false;
  }
  data = favicon_png_.data();
  data_size = favicon_png_.size();
  return true;
}
