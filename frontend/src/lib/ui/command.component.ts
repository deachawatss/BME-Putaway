import { Component, Input, Output, EventEmitter, TemplateRef, ContentChild, OnInit, OnDestroy, ElementRef, ViewChild } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { cn } from '../utils';

@Component({
  selector: 'app-command',
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <div [class]="cn('bg-popover text-popover-foreground flex h-full w-full flex-col overflow-hidden rounded-md', className)">
      <ng-content></ng-content>
    </div>
  `
})
export class CommandComponent {
  @Input() className: string = '';
  
  cn = cn;
}

@Component({
  selector: 'app-command-input',
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <div class="flex h-9 items-center gap-2 border-b px-3">
      <svg class="size-4 shrink-0 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
      </svg>
      <input
        #inputElement
        [(ngModel)]="value"
        (input)="onInput($event)"
        (keydown)="onKeyDown($event)"
        [placeholder]="placeholder"
        [class]="cn('placeholder:text-muted-foreground flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-hidden disabled:cursor-not-allowed disabled:opacity-50', className)"
        [disabled]="disabled"
      />
    </div>
  `
})
export class CommandInputComponent implements OnInit {
  @Input() placeholder: string = '';
  @Input() className: string = '';
  @Input() disabled: boolean = false;
  @Input() value: string = '';
  @Output() valueChange = new EventEmitter<string>();
  @Output() inputChange = new EventEmitter<string>();
  @Output() keydown = new EventEmitter<KeyboardEvent>();
  
  @ViewChild('inputElement') inputElement!: ElementRef;
  
  cn = cn;

  ngOnInit() {
    setTimeout(() => {
      if (this.inputElement) {
        this.inputElement.nativeElement.focus();
      }
    }, 100);
  }

  onInput(event: Event) {
    const target = event.target as HTMLInputElement;
    this.value = target.value;
    this.valueChange.emit(this.value);
    this.inputChange.emit(this.value);
  }

  onKeyDown(event: KeyboardEvent) {
    this.keydown.emit(event);
  }
}

@Component({
  selector: 'app-command-list',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div [class]="cn('max-h-[300px] scroll-py-1 overflow-x-hidden overflow-y-auto', className)">
      <ng-content></ng-content>
    </div>
  `
})
export class CommandListComponent {
  @Input() className: string = '';
  
  cn = cn;
}

@Component({
  selector: 'app-command-empty',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div class="py-6 text-center text-sm">
      <ng-content></ng-content>
    </div>
  `
})
export class CommandEmptyComponent {}

@Component({
  selector: 'app-command-group',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div [class]="cn('text-foreground overflow-hidden p-1', className)">
      <div *ngIf="heading" class="text-muted-foreground px-2 py-1.5 text-xs font-medium">
        {{ heading }}
      </div>
      <ng-content></ng-content>
    </div>
  `
})
export class CommandGroupComponent {
  @Input() heading: string = '';
  @Input() className: string = '';
  
  cn = cn;
}

@Component({
  selector: 'app-command-item',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div 
      (click)="onClick()"
      [class]="cn(
        'relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground',
        disabled ? 'pointer-events-none opacity-50' : '',
        selected ? 'bg-accent text-accent-foreground' : '',
        className
      )">
      <ng-content></ng-content>
      <span *ngIf="shortcut" class="text-muted-foreground ml-auto text-xs tracking-widest">
        {{ shortcut }}
      </span>
    </div>
  `
})
export class CommandItemComponent<T = unknown> {
  @Input() className: string = '';
  @Input() disabled: boolean = false;
  @Input() selected: boolean = false;
  @Input() shortcut: string = '';
  @Input() value: T | undefined;
  @Output() select = new EventEmitter<T>();
  
  cn = cn;

  onClick() {
    if (!this.disabled && this.value !== undefined) {
      this.select.emit(this.value);
    }
  }
}

@Component({
  selector: 'app-command-separator',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div [class]="cn('bg-border -mx-1 h-px', className)"></div>
  `
})
export class CommandSeparatorComponent {
  @Input() className: string = '';
  
  cn = cn;
}