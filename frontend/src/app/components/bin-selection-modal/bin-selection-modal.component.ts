import { Component, EventEmitter, Input, Output, signal, effect, inject, input } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormControl, ReactiveFormsModule } from '@angular/forms';
import { debounceTime, distinctUntilChanged } from 'rxjs/operators';

import { PutawayService, BinSearchItem, PaginatedBinSearchResponse } from '../../services/putaway.service';

@Component({
  selector: 'app-bin-selection-modal',
  standalone: true,
  imports: [CommonModule, ReactiveFormsModule],
  template: `
    <!-- Modal Overlay -->
    <div 
      *ngIf="isOpen()" 
      class="tw-fixed tw-inset-0 tw-z-50 tw-flex tw-items-center tw-justify-center tw-bg-black/50 tw-p-4"
      (click)="onOverlayClick($event)">
      
      <!-- Modal Dialog -->
      <div 
        class="tw-bg-white tw-rounded-lg tw-shadow-xl tw-w-full tw-max-w-sm sm:tw-max-w-md md:tw-max-w-2xl lg:tw-max-w-4xl xl:tw-max-w-6xl tw-max-h-[90vh] tw-mx-2 sm:tw-mx-4 tw-flex tw-flex-col"
        (click)="$event.stopPropagation()">
        
        <!-- Modal Header -->
        <div class="nwfth-button-primary tw-p-4 tw-rounded-t-lg">
          <div class="tw-flex tw-items-center tw-justify-between">
            <h2 class="tw-text-xl tw-font-bold tw-text-white tw-flex tw-items-center tw-gap-3">
              <span class="tw-text-2xl">📦</span>
              <span>Select Bin Number</span>
            </h2>
            <button 
              type="button"
              (click)="onClose()"
              class="tw-bg-white/20 hover:tw-bg-white/30 tw-text-white tw-p-2 tw-rounded-lg tw-transition-all tw-duration-200 hover:tw-scale-105"
              aria-label="Close dialog">
              ✕
            </button>
          </div>
        </div>

        <!-- Search Section -->
        <div class="tw-p-4 tw-border-b tw-border-gray-200">
          <div class="tw-relative">
            <input
              type="text"
              [formControl]="searchControl"
              placeholder="Search bins by number, location, or description..."
              class="nwfth-input tw-w-full tw-pr-12 tw-px-3 tw-py-2"
              autofocus
            />
            <div class="tw-absolute tw-right-3 tw-top-1/2 tw-transform tw--translate-y-1/2">
              <div *ngIf="isSearching()" class="tw-w-5 tw-h-5 tw-border-2 tw-border-amber-500 tw-border-t-transparent tw-rounded-full tw-animate-spin"></div>
              <span *ngIf="!isSearching()" class="tw-text-gray-400">🔍</span>
            </div>
          </div>
        </div>

        <!-- Results Section -->
        <div class="tw-flex-1 tw-overflow-auto">
          <!-- Loading State -->
          <div *ngIf="isSearching()" class="tw-p-8 tw-text-center">
            <div class="tw-w-8 tw-h-8 tw-border-4 tw-border-amber-500 tw-border-t-transparent tw-rounded-full tw-animate-spin tw-mx-auto tw-mb-4"></div>
            <p class="tw-text-gray-600">Searching bins...</p>
          </div>

          <!-- Empty State -->
          <div *ngIf="!isSearching() && searchResults().length === 0" class="tw-p-8 tw-text-center">
            <span class="tw-text-6xl tw-text-gray-300 tw-block tw-mb-4">📦</span>
            <p class="tw-text-gray-600 tw-mb-2">{{ hasSearched() ? 'No bins found' : 'Start typing to search bins' }}</p>
            <p class="tw-text-gray-400 tw-text-sm">{{ hasSearched() ? 'Try a different search term' : 'Enter bin number, location, or description' }}</p>
          </div>

          <!-- Results - Desktop/Tablet Table View (md and up) -->
          <div *ngIf="!isSearching() && searchResults().length > 0" class="tw-hidden md:tw-block tw-overflow-x-auto tw-min-w-0">
            <table class="tw-w-full md:tw-min-w-[500px] lg:tw-min-w-[700px] tw-text-sm tw-table-auto">
              <thead class="tw-bg-gray-50 tw-border-b tw-border-gray-200 tw-sticky tw-top-0">
                <tr>
                  <th class="tw-px-2 md:tw-px-3 tw-py-3 tw-text-left tw-font-semibold tw-text-gray-700 tw-min-w-[100px] md:tw-min-w-[120px]">Bin #</th>
                  <th class="tw-px-2 md:tw-px-3 tw-py-3 tw-text-left tw-font-semibold tw-text-gray-700 tw-min-w-[80px] md:tw-min-w-[100px]">Location</th>
                  <th class="tw-px-2 md:tw-px-3 tw-py-3 tw-text-left tw-font-semibold tw-text-gray-700 tw-min-w-[150px] md:tw-min-w-[300px]">Description</th>
                  <th class="tw-px-2 md:tw-px-3 tw-py-3 tw-text-center tw-font-semibold tw-text-gray-700 tw-min-w-[80px] md:tw-min-w-[100px]">Action</th>
                </tr>
              </thead>
              <tbody class="tw-divide-y tw-divide-gray-200">
                <tr 
                  *ngFor="let bin of searchResults(); trackBy: trackByBinNo"
                  class="hover:tw-bg-gray-50 tw-transition-colors tw-duration-150">
                  <td class="tw-px-2 md:tw-px-3 tw-py-3 tw-font-medium tw-text-gray-900 tw-break-words tw-text-xs md:tw-text-sm">{{ bin.bin_no }}</td>
                  <td class="tw-px-2 md:tw-px-3 tw-py-3 tw-text-gray-700 tw-text-xs md:tw-text-sm">{{ bin.location }}</td>
                  <td class="tw-px-2 md:tw-px-3 tw-py-3 tw-text-gray-700 tw-text-xs md:tw-text-sm" [title]="bin.description">
                    <div class="tw-max-w-[120px] md:tw-max-w-[300px] tw-truncate">{{ bin.description }}</div>
                  </td>
                  <td class="tw-px-2 md:tw-px-3 tw-py-3 tw-text-center">
                    <button
                      type="button"
                      (click)="onSelectBin(bin)"
                      class="nwfth-button-primary tw-px-2 md:tw-px-3 tw-py-1 md:tw-py-2 tw-text-xs md:tw-text-sm tw-rounded-lg tw-transition-all tw-duration-200 hover:tw-bg-opacity-90 tw-whitespace-nowrap tw-min-w-[50px] md:tw-min-w-[70px]">
                      Select
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- Results - Mobile Card View (sm and down) -->
          <div *ngIf="!isSearching() && searchResults().length > 0" class="tw-block md:tw-hidden tw-p-4 tw-space-y-3">
            <div 
              *ngFor="let bin of searchResults(); trackBy: trackByBinNo"
              class="tw-bg-white tw-border tw-border-gray-200 tw-rounded-lg tw-p-4 tw-shadow-sm hover:tw-shadow-md tw-transition-shadow">
              
              <!-- Mobile Card Header -->
              <div class="tw-flex tw-justify-between tw-items-start tw-mb-3">
                <div class="tw-flex-1">
                  <h4 class="tw-font-bold tw-text-gray-900 tw-text-lg">{{ bin.bin_no }}</h4>
                  <p class="tw-text-sm tw-text-gray-600 tw-font-medium">{{ bin.location }}</p>
                </div>
                <button
                  type="button"
                  (click)="onSelectBin(bin)"
                  class="nwfth-button-primary tw-px-4 tw-py-2 tw-text-sm tw-rounded-lg tw-transition-all tw-duration-200 hover:tw-bg-opacity-90 tw-ml-3 tw-flex-shrink-0">
                  Select
                </button>
              </div>

              <!-- Mobile Card Details -->
              <div class="tw-space-y-2 tw-text-sm">
                <div class="tw-flex tw-justify-between">
                  <span class="tw-text-gray-500">Description:</span>
                  <span class="tw-text-gray-900 tw-text-right tw-max-w-[200px] tw-truncate" [title]="bin.description">
                    {{ bin.description }}
                  </span>
                </div>
                <div *ngIf="bin.aisle || bin.row || bin.rack" class="tw-flex tw-justify-between">
                  <span class="tw-text-gray-500">Position:</span>
                  <span class="tw-text-gray-900 tw-font-mono tw-text-xs">
                    {{ getPositionDisplay(bin) }}
                  </span>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Pagination -->
        <div *ngIf="!isSearching() && searchResults().length > 0 && totalPages() > 1" 
             class="tw-px-4 tw-py-3 tw-border-t tw-border-gray-200">
          <div class="tw-flex tw-items-center tw-justify-between tw-flex-wrap tw-gap-2">
            <!-- Page Info -->
            <div class="tw-text-sm tw-text-gray-600 tw-order-3 sm:tw-order-1">
              Showing {{ ((currentPage() - 1) * pageLimit()) + 1 }} to {{ Math.min(currentPage() * pageLimit(), totalResults()) }} of {{ totalResults() }} results
            </div>
            
            <!-- Pagination Controls -->
            <div class="tw-flex tw-items-center tw-gap-1 tw-order-1 sm:tw-order-2">
              <!-- Previous Button -->
              <button
                type="button"
                (click)="goToPage(currentPage() - 1)"
                [disabled]="currentPage() <= 1"
                class="tw-px-3 tw-py-2 tw-text-sm tw-border tw-border-gray-300 tw-rounded-lg tw-transition-all tw-duration-200 tw-disabled:opacity-50 tw-disabled:cursor-not-allowed hover:tw-bg-gray-50 enabled:hover:tw-border-gray-400">
                ‹ Prev
              </button>
              
              <!-- Page Numbers -->
              <div class="tw-flex tw-items-center tw-gap-1">
                <button
                  *ngFor="let page of getVisiblePages()"
                  type="button"
                  (click)="goToPage(page)"
                  [class]="page === currentPage() 
                    ? 'nwfth-button-primary tw-px-3 tw-py-2 tw-text-sm tw-rounded-lg tw-min-w-[40px]'
                    : 'tw-px-3 tw-py-2 tw-text-sm tw-border tw-border-gray-300 tw-rounded-lg tw-transition-all tw-duration-200 hover:tw-bg-gray-50 hover:tw-border-gray-400 tw-min-w-[40px] tw-text-center'">
                  {{ page }}
                </button>
              </div>
              
              <!-- Next Button -->
              <button
                type="button"
                (click)="goToPage(currentPage() + 1)"
                [disabled]="currentPage() >= totalPages()"
                class="tw-px-3 tw-py-2 tw-text-sm tw-border tw-border-gray-300 tw-rounded-lg tw-transition-all tw-duration-200 tw-disabled:opacity-50 tw-disabled:cursor-not-allowed hover:tw-bg-gray-50 enabled:hover:tw-border-gray-400">
                Next ›
              </button>
            </div>
          </div>
        </div>

        <!-- Modal Footer -->
        <div class="tw-p-3 md:tw-p-4 tw-border-t tw-border-gray-200">
          <div class="tw-flex tw-flex-col-reverse sm:tw-flex-row tw-gap-3 sm:tw-justify-between sm:tw-items-center">
            <p class="tw-text-xs md:tw-text-sm tw-text-gray-600 tw-text-center sm:tw-text-left">
              {{ totalResults() }} bin{{ totalResults() !== 1 ? 's' : '' }} found{{ totalPages() > 1 ? ' (page ' + currentPage() + ' of ' + totalPages() + ')' : '' }}
            </p>
            <button
              type="button"
              (click)="onClose()"
              class="nwfth-button-secondary tw-px-4 md:tw-px-6 tw-py-2 tw-w-full sm:tw-w-auto tw-text-sm md:tw-text-base">
              Cancel
            </button>
          </div>
        </div>

      </div>
    </div>
  `
})
export class BinSelectionModalComponent {
  @Input() isOpen = signal(false);
  initialFilter = input<string>(''); // Convert to signal-based input
  lotContext = input<{ lot_no: string; item_key: string; location: string } | undefined>(undefined); // Lot context for status display
  @Output() binSelected = new EventEmitter<BinSearchItem>();
  @Output() modalClosed = new EventEmitter<void>();

  private putawayService = inject(PutawayService);

  // State signals
  searchResults = signal<BinSearchItem[]>([]);
  isSearching = signal(false);
  hasSearched = signal(false);
  errorMessage = signal<string>('');
  userHasSearched = signal(false); // Track if user has manually changed search
  hasAppliedInitialFilter = signal(false); // Track if initial filter has been applied
  
  // Pagination signals
  currentPage = signal(1);
  totalPages = signal(1);
  totalResults = signal(0);
  pageLimit = signal(20);

  // Form controls
  searchControl = new FormControl('');

  // Math reference for template
  Math = Math;

  constructor() {
    // Setup search with debouncing
    effect(() => {
      this.searchControl.valueChanges.pipe(
        debounceTime(300),
        distinctUntilChanged()
      ).subscribe(query => {
        this.userHasSearched.set(true); // Mark that user has manually changed search
        this.currentPage.set(1); // Reset to first page on search
        this.performSearch(query || '', 1);
      });
    });

    // Load initial results when modal opens
    effect(() => {
      if (this.isOpen()) {
        this.loadInitialResults();
      }
    });

    // Set initial filter when provided (only once per modal session)
    effect(() => {
      const filter = this.initialFilter(); // Use signal getter
      if (filter && this.isOpen() && !this.userHasSearched() && !this.hasAppliedInitialFilter()) {
        this.searchControl.setValue(filter, { emitEvent: false });
        this.currentPage.set(1);
        this.performSearch(filter, 1);
        this.hasAppliedInitialFilter.set(true); // Mark initial filter as applied
      }
    });
  }

  private loadInitialResults() {
    // Reset state when modal opens
    this.userHasSearched.set(false);
    this.hasAppliedInitialFilter.set(false); // Reset initial filter application flag
    this.currentPage.set(1);
    
    // Only perform empty search here - let the effect handle initial filter application
    // This prevents duplicate logic and race conditions
    if (!this.initialFilter()) {
      this.performSearch('', 1);
    }
  }

  private async performSearch(query: string, page: number = 1) {
    this.isSearching.set(true);
    this.errorMessage.set('');
    this.hasSearched.set(true);

    try {
      const response = await this.putawayService.searchBinsWithPagination(
        query || undefined,
        page,
        this.pageLimit(),
        this.lotContext() // Pass lot context for status display
      ).toPromise();
      
      if (response) {
        this.searchResults.set(response.items || []);
        this.currentPage.set(response.page);
        this.totalPages.set(response.pages);
        this.totalResults.set(response.total);
      } else {
        this.searchResults.set([]);
        this.currentPage.set(1);
        this.totalPages.set(1);
        this.totalResults.set(0);
      }
    } catch (error) {
      console.error('Error searching bins:', error);
      this.errorMessage.set('Error searching bins. Please try again.');
      this.searchResults.set([]);
      this.currentPage.set(1);
      this.totalPages.set(1);
      this.totalResults.set(0);
    } finally {
      this.isSearching.set(false);
    }
  }

  onSelectBin(bin: BinSearchItem) {
    this.binSelected.emit(bin);
  }

  onClose() {
    // Reset search state when modal closes
    this.userHasSearched.set(false);
    this.hasAppliedInitialFilter.set(false); // Reset initial filter flag
    this.searchControl.setValue('', { emitEvent: false });
    this.currentPage.set(1);
    this.modalClosed.emit();
  }

  onOverlayClick(event: MouseEvent) {
    // Close modal when clicking on overlay (not the dialog content)
    this.onClose();
  }

  trackByBinNo(index: number, bin: BinSearchItem): string {
    return bin.bin_no + bin.location;
  }

  // Format position display for mobile cards
  getPositionDisplay(bin: BinSearchItem): string {
    const parts = [];
    if (bin.aisle) parts.push(`A:${bin.aisle}`);
    if (bin.row) parts.push(`R:${bin.row}`);
    if (bin.rack) parts.push(`L:${bin.rack}`);
    return parts.join(' ') || 'N/A';
  }

  // Pagination methods
  goToPage(page: number) {
    if (page >= 1 && page <= this.totalPages() && page !== this.currentPage()) {
      const query = this.searchControl.value || '';
      this.performSearch(query, page);
    }
  }

  getVisiblePages(): number[] {
    const current = this.currentPage();
    const total = this.totalPages();
    const pages: number[] = [];
    
    if (total <= 7) {
      // Show all pages if 7 or fewer
      for (let i = 1; i <= total; i++) {
        pages.push(i);
      }
    } else {
      // Smart pagination for many pages
      if (current <= 4) {
        // Show first 5 pages, then last page
        for (let i = 1; i <= 5; i++) {
          pages.push(i);
        }
        if (total > 6) pages.push(-1); // Ellipsis marker
        pages.push(total);
      } else if (current >= total - 3) {
        // Show first page, then last 5 pages
        pages.push(1);
        if (total > 6) pages.push(-1); // Ellipsis marker
        for (let i = total - 4; i <= total; i++) {
          pages.push(i);
        }
      } else {
        // Show first page, current-1, current, current+1, last page
        pages.push(1);
        pages.push(-1); // Ellipsis marker
        for (let i = current - 1; i <= current + 1; i++) {
          pages.push(i);
        }
        pages.push(-1); // Ellipsis marker
        pages.push(total);
      }
    }
    
    return pages.filter(p => p > 0); // Remove ellipsis markers for now
  }
}